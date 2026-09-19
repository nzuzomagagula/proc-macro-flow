// @review [ ]
//! The derives: generating extraction logic instead of writing it out.
//!
//! NOTE(#derive/cannot-self-host): V[N(proc_macro_flow_derive).!uses(Attr(derive(Extractor)))],
//! "VERIFIED: `can't use a procedural macro from the same crate that defines it`. So
//! StructExtraction and FieldExtraction - which live here - can NEVER carry these derives, and the
//! plan's 'rewrite the existing extractors onto the derive' step is not merely unfinished but
//! impossible where they sit. They stay hand-written as the bootstrap, and the derive is proved
//! from proc_macro_flow instead: the facade depends on both crates, so it can host the equivalent
//! extraction and assert the generated tree matches the hand-written one. Same proof, a crate over"
//!
//! NOTE(#derive/three-not-one): V[Attr(derive(Extractor)) != Attr(derive(Validate))], "Three
//! separate derives rather than one that emits everything, because the cases genuinely differ.
//! StructExtraction has a REAL validate - DeriveInput narrowed to &DataStruct - so a derive that
//! always emitted a trivial one would be unusable for exactly the type that motivated the design.
//! Deriving what you want generated and hand-writing the rest is ordinary Rust and needs no opt-out
//! attribute, which is the same reasoning ID(processor/optionality) already settled"

mod extractor;
mod processor;
mod validate;

pub(crate) use extractor::derive_extractor;
pub(crate) use processor::derive_processor;
pub(crate) use validate::derive_validate;

use syn::{
    Attribute, Error, GenericArgument, PathArguments, Result, Type, spanned::Spanned,
};

/// How many children a field declares, read off its written type.
///
/// This is `#from/arity-from-type`, and it is where a proc macro beats `macro_rules!`: the type is
/// *parsed*, so `std::option::Option<T>` and `Option<T>` are the same thing here, where a
/// declarative macro could only match the tokens it was handed.
pub(crate) enum Arity {
    /// `Extracted<T, I>` — exactly one.
    One,
    /// `Vec<Extracted<T, I>>` — many.
    Many,
    /// `Option<Extracted<T, I>>` — absence is not a failure.
    Maybe,
}

/// A field's arity and the extractor that produces its children.
pub(crate) struct Child {
    pub(crate) arity: Arity,
    /// The `T` of `Extracted<T, I>`, which the generated call must name — `T::Output` is an
    /// associated type and so not inferable (`#from/names-its-target`).
    pub(crate) extractor: Type,
}

impl Child {
    pub(crate) fn of(ty: &Type) -> Result<Self> {
        if let Some(inner) = unwrap_generic(ty, "Vec") {
            return Ok(Child {
                arity: Arity::Many,
                extractor: extractor_of(inner)?,
            });
        }
        if let Some(inner) = unwrap_generic(ty, "Option") {
            return Ok(Child {
                arity: Arity::Maybe,
                extractor: extractor_of(inner)?,
            });
        }
        Ok(Child {
            arity: Arity::One,
            extractor: extractor_of(ty)?,
        })
    }
}

/// The `T` in `Extracted<T, I>`.
fn extractor_of(ty: &Type) -> Result<Type> {
    unwrap_generic(ty, "Extracted").cloned().ok_or_else(|| {
        Error::new(
            ty.span(),
            "expected `Extracted<T, I>`, optionally inside `Vec` or `Option` - a field with \
             `#[from]` holds what its child extractor produced",
        )
    })
}

/// The first generic argument of `Name<..>`, when the type's last segment is `Name`.
fn unwrap_generic<'ty>(ty: &'ty Type, name: &str) -> Option<&'ty Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;

    if segment.ident != name {
        return None;
    }

    let PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };

    args.args.iter().find_map(|arg| match arg {
        GenericArgument::Type(inner) => Some(inner),
        _ => None,
    })
}

/// Read the single expression out of `#[name(expr)]`.
///
/// NOTE(#derive/list-not-name-value): V[Attr(from).list], "`#[from(expr)]` and NOT
/// `#[from = expr]`, and this is forced rather than chosen. VERIFIED: rustc rejects the name-value
/// form with `attribute value must be a literal` - after `=` a derive helper attribute may carry a
/// literal and nothing else, so `#[from = source.fields.iter()]` never reaches the macro at all.
/// The LIST form takes arbitrary tokens, which is the same property the whole design already rests
/// on: MetaList::tokens is raw and unparsed (ID(openings)). So the one place the framework needs to
/// carry an un-inspected expression is the one place syn's grammar leaves open for it"
pub(crate) fn expr_arg(attr: &Attribute) -> Result<syn::Expr> {
    attr.parse_args::<syn::Expr>().map_err(|_| {
        Error::new(
            attr.span(),
            "expected `(<expression>)` - the expression is spliced verbatim and never inspected",
        )
    })
}

/// Find at most one attribute with the given head.
pub(crate) fn find_one<'a>(attrs: &'a [Attribute], name: &str) -> Result<Option<&'a Attribute>> {
    let mut found = attrs.iter().filter(|a| a.path().is_ident(name));
    let first = found.next();

    if let Some(extra) = found.next() {
        return Err(Error::new(
            extra.span(),
            format!("`{name}` is written more than once"),
        ));
    }

    Ok(first)
}
