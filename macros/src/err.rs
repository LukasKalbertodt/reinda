use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;


pub(crate) struct Error {
    pub(crate) msg: String,
    pub(crate) span: Option<Span>,
}

impl Error {
    pub(crate) fn to_compile_error(&self) -> TokenStream {
        let span = self.span.unwrap_or(Span::call_site());
        let msg = &self.msg;
        quote_spanned! {span=> compile_error!(#msg);}
    }
}

macro_rules! err {
    (@$span:expr, $fmt:literal $($t:tt)*) => {
        $crate::err::Error {
            msg: format!($fmt $($t)*),
            span: Some($span.clone()),
        }
    };
    ($fmt:literal $($t:tt)*) => {
        $crate::err::Error {
            msg: format!($fmt $($t)*),
            span: None,
        }
    };
}

pub(crate) use err;
