// @review [ ]
//! The generation stage: emitting code, and emitting it even when something went wrong.
//!
//! NOTE(#generator/stub-alongside-errors): V[F(emit).always_includes(stub)], "A generator MUST emit
//! its stub even when the extraction failed, and this is not a stylistic preference. A generator
//! that returns only compile_error!s leaves the impl missing, and every use site then reports
//! 'does not implement ..' - N follow-on errors that bury the one real diagnostic underneath them.
//! The floor is therefore a stub item BESIDE the errors, always. `emit` below exists so that is
//! mechanical rather than remembered, the same way walk_keys owns unknown-key reporting instead of
//! trusting each caller to do it"
//!
//! NOTE(#generator/one-input-shape): V[Ty(Input) == Tr(Processor).Ty(Output)], "A generator consumes
//! a processor's Output and nothing else. That is only ONE shape because processing is never
//! skipped - an extraction with no real processing derives the identity Processor, so the stage
//! always ran. Making the generator accept 'either an extraction or a processed value' was the
//! alternative, and it would have put that either-ness in every downstream signature forever"

use proc_macro2::TokenStream;
use quote::ToTokens;

use crate::extractor::Reason;

/// Emit code from a processed value.
pub trait Generator: Sized {
    /// A processor's `Output`.
    type Input;

    fn generate(input: Self::Input) -> TokenStream;
}

/// Combine a stub with the reasons that were recorded against it.
///
/// This is the shape every generator's output should take: the stub FIRST so it exists whatever
/// else happened, then one `compile_error!` per reason. `node` is the fallback a reason spans
/// against when it has nothing finer to point at.
pub fn emit(stub: TokenStream, reasons: &[Reason], node: &impl ToTokens, message: &str) -> TokenStream {
    let mut out = stub;
    out.extend(
        reasons
            .iter()
            .map(|reason| reason.to_error(node, message).to_compile_error()),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractor::{Reason, ReasonKind};
    use quote::quote;

    #[test]
    fn the_stub_survives_the_errors() {
        // THE property. Without this the missing impl cascades at every use site.
        let stub = quote!(impl Thing for T {});
        let reasons = vec![
            Reason::new(ReasonKind::Missing),
            Reason::new(ReasonKind::UnknownKey),
        ];

        let out = emit(stub, &reasons, &quote!(node), "bad").to_string();

        assert!(out.contains("impl Thing for T"), "{out}");
        assert_eq!(out.matches("compile_error").count(), 2, "{out}");
    }

    #[test]
    fn a_clean_generation_is_just_the_stub() {
        let out = emit(quote!(impl Thing for T {}), &[], &quote!(node), "bad").to_string();

        assert!(out.contains("impl Thing for T"));
        assert!(!out.contains("compile_error"));
    }

    #[test]
    fn the_stub_comes_first() {
        // Order matters for readability of the emitted file, and for anyone reading expansion.
        let out = emit(
            quote!(impl Thing for T {}),
            &[Reason::new(ReasonKind::Missing)],
            &quote!(node),
            "bad",
        )
        .to_string();

        assert!(out.find("impl").unwrap() < out.find("compile_error").unwrap(), "{out}");
    }
}
