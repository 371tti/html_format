//! html_format_macro/src/lib.rs
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream},
    punctuated::Punctuated, Expr, Ident, LitStr, Token,
};

struct Input {
    tpl: LitStr,
    pairs: Punctuated<(Ident, Expr), Token![,]>,
}

impl Parse for Input {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let tpl: LitStr = input.parse()?;
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

fn parse_template(raw: &str) -> (Vec<String>, Vec<String>, usize) {
    let mut lit_parts = Vec::<String>::new();
    let mut keys      = Vec::<String>::new();
    let mut buf = String::new();

    let bytes = raw.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            // flush literal
            if !buf.is_empty() {
                lit_parts.push(std::mem::take(&mut buf));
            }
            i += 2;
            while i < bytes.len() && bytes[i].is_ascii_whitespace() { i += 1; }
            let start = i;
            while i < bytes.len() && bytes[i] != b'}' { i += 1; }
            keys.push(raw[start..i].trim().to_string());
            i += 2; // skip "}}"
        } else {
            // escape single { }
            match bytes[i] {
                b'{' => buf.push_str("{{"),
                b'}' => buf.push_str("}}"),
                _    => buf.push(bytes[i] as char),
            }
            i += 1;
        }
    }
    if !buf.is_empty() {
        lit_parts.push(buf);
    }

    let lit_len = lit_parts.iter().map(|s| s.len()).sum();
    (lit_parts, keys, lit_len)
}

#[proc_macro]
pub fn html_format(input: TokenStream) -> TokenStream {
    let Input { tpl, pairs } = syn::parse_macro_input!(input as Input);

    let raw = tpl.value();
    let (lit_parts, keys_in_tpl, lit_len) = parse_template(&raw);

    let mut val_map = std::collections::HashMap::<String, TokenStream2>::new();
    for (id, expr) in pairs {
        val_map.insert(id.to_string(), expr.into_token_stream());
    }

    let mut vals_ts = Vec::<TokenStream2>::new();
    for k in &keys_in_tpl {
        match val_map.get(k) {
            Some(t) => vals_ts.push(t.clone()),
            None => {
                return syn::Error::new_spanned(
                    &tpl,
                    format!("missing value for key `{}`", k),
                )
                .to_compile_error()
                .into();
            }
        }
    }

    let lit_tokens: Vec<LitStr> = lit_parts
        .iter()
        .map(|s| LitStr::new(s, tpl.span()))
        .collect();

    let mut interleave = TokenStream2::new();
    for (lit, val) in lit_tokens.iter().take(vals_ts.len()).zip(vals_ts.iter()) {
        interleave.extend(quote! {
            s.push_str(#lit);
            s.push_str(&::std::string::ToString::to_string(&(#val)));
        });
    }
    let last_lit = lit_tokens.last().unwrap();

    let cap = lit_len + 16 * vals_ts.len();

    let expanded = quote! {{
        let mut s = ::std::string::String::with_capacity(#cap);
        #interleave
        s.push_str(#last_lit);
        s
    }};
    TokenStream::from(expanded)
}
