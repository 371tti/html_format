use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated, Expr, Ident, LitStr, Token,
};

struct Input {
    tpl: Expr,
    pairs: Punctuated<(Ident, Expr), Token![,]>,
}
impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let tpl: Expr = input.parse()?;
        let _ = input.parse::<Option<Token![,]>>();
        let pairs = Punctuated::<(Ident, Expr), Token![,]>::parse_terminated_with(
            input,
            |s| {
                let k: Ident = s.parse()?;
                s.parse::<Token![=]>()?;
                let v: Expr = s.parse()?;
                Ok((k, v))
            },
        )?;
        Ok(Self { tpl, pairs })
    }
}

fn parse_literal_tpl(raw: &str) -> (Vec<String>, Vec<String>, usize) {
    let mut lit = Vec::new();
    let mut keys = Vec::new();
    let mut buf = String::new();
    let mut chars = raw.char_indices().peekable();
    let mut last_i = 0;

    while let Some((i, c)) = chars.next() {
        if c == '{' {
            if let Some(&(_, '{')) = chars.peek() {
                chars.next(); // skip second {
                if !buf.is_empty() {
                    lit.push(std::mem::take(&mut buf));
                }
                // skip whitespace (including full-width)
                while let Some(&(_, ch)) = chars.peek() {
                    if ch.is_whitespace() {
                        chars.next();
                    } else {
                        break;
                    }
                }

                // collect key until `}`
                let start = chars.peek().map(|(i, _)| *i).unwrap_or(i);
                let mut end = start;
                while let Some(&(j, ch)) = chars.peek() {
                    if ch == '}' {
                        end = j;
                        break;
                    }
                    chars.next();
                }

                // skip closing "}}"
                chars.next(); // consume first }
                chars.next(); // consume second }

                let key = raw[start..end].trim().to_string();
                keys.push(key);
                last_i = end + 2;
                continue;
            }
        }

        match c {
            '{' => buf.push_str("{{"),
            '}' => buf.push_str("}}"),
            _ => buf.push(c),
        }
        last_i = i + c.len_utf8();
    }

    if !buf.is_empty() {
        lit.push(buf);
    }

    let len = lit.iter().map(|s| s.len()).sum();
    (lit, keys, len)
}


#[proc_macro]
pub fn html_format(input: TokenStream) -> TokenStream {
    let Input { tpl, pairs } = syn::parse_macro_input!(input as Input);

    let (keys_vec, vals_ts): (Vec<String>, Vec<TokenStream2>) = pairs
        .iter()
        .map(|(id, ex)| (id.to_string(), ex.into_token_stream()))
        .unzip();

    let r#gen: TokenStream2 = match &tpl {
        syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(lit_str), .. }) => {
            let (lit_parts, key_order, lit_len) = parse_literal_tpl(&lit_str.value());

            let mut ordered_vals = Vec::<TokenStream2>::new();
            for k in &key_order {
                match keys_vec.iter().position(|kk| kk == k) {
                    Some(idx) => ordered_vals.push(vals_ts[idx].clone()),
                    None => {
                        return syn::Error::new_spanned(
                            &tpl, format!("missing value for key `{}`", k)
                        ).to_compile_error().into()
                    }
                }
            }

            let lit_tokens: Vec<LitStr> =
                lit_parts.iter().map(|s| LitStr::new(s, lit_str.span())).collect();
            let last = lit_tokens.last().unwrap();
            let inter = &lit_tokens[..lit_tokens.len() - 1];
            let cap = lit_len + 16 * ordered_vals.len();

            quote! {{
                let mut s = ::std::string::String::with_capacity(#cap);
                #( {
                    s.push_str(#inter);
                    s.push_str(&::std::string::ToString::to_string(&(#ordered_vals)));
                } )*
                s.push_str(#last);
                s
            }}
        }

        _ => {
            let arms: Vec<TokenStream2> = keys_vec.iter().zip(vals_ts.iter())
                .map(|(k, v)| {
                    quote! { #k => s.push_str(&::std::string::ToString::to_string(&(#v))), }
                })
                .collect();

            quote! {{
                let raw = (#tpl) as &str;
                let mut s = ::std::string::String::with_capacity(raw.len() + 16);
                let b = raw.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'{' && i + 1 < b.len() && b[i+1] == b'{' {
                        i += 2;
                        while i < b.len() && b[i].is_ascii_whitespace() { i += 1; }
                        let start = i;
                        while i < b.len() && b[i] != b'}' { i += 1; }
                        let key = raw[start..i].trim();
                        match key {
                            #(#arms)*
                            _ => { s.push_str("{{"); s.push_str(key); s.push_str("}}"); }
                        }
                        i += 2;
                    } else {
                        match b[i] {
                            b'{' => s.push_str("{{"),
                            b'}' => s.push_str("}}"),
                            _    => s.push(b[i] as char),
                        }
                        i += 1;
                    }
                }
                s
            }}
        }
    };

    TokenStream::from(r#gen)
}
