// @review [ ]
//! Reading a `#[pipeline]` module: which items are stages, and which is the helper vocabulary.
//!
// NOTE(#pipeline-macro/dogfoods-the-pattern): V[N(pipeline).has(extractor, processor, generator)] && V[Impl(Pipeline).for(PipelineWiring)], "The macro that writes macros is written as a pipeline"
// The macro that writes macros is itself written as extractor -> processor -> generator, and now
// RUNS through Tr(Pipeline)::run like any other. That is dogfooding with a purpose rather than
// symmetry for its own sake: if the pattern could not express its own macro, that is a finding
// worth having early.
//
// It did not always. This module used to drive the three stages by hand, and the exception was
// recorded as a real limitation - see ID(pipeline/subject-equals-source-breaks-attributes)
// for the finding that the limitation was never there

use proc_macro_flow_traits::extractor::{Extracted, Extraction, Extractor, Reason, ReasonKind, Validate};
use proc_macro_flow_traits::assert::Assert;
use proc_macro_flow_traits::attributed::Attributed;
use proc_macro_flow_traits::render::Diagnose;
use syn::{Item, ItemMod};

use crate::derive::ext::AttributesExt;

use super::generator::PipelineArgs;

proc_macro_flow_traits::vocabulary! {
    /// The three roles an item in a pipeline module may declare.
    ///
    /// A closed set, matched from the attribute head - so an item wearing none of these is simply
    /// not a stage, and that is not a complaint (ID(heads-are-rustcs)).
    pub enum Role {
        Extractor = "extractor",
        Processor = "processor",
        Generator = "generator",
    }
}

proc_macro_flow_traits::vocabulary! {
    /// What a generator does to the item its attribute macro was applied to.
    ///
    /// NOTE(#pipeline-macro/emission-must-be-declared): V[E(Emission).is(declared)], "A generator that rewrites the item must declare it"
    /// F(run_attribute) hands the annotated item back and the generator adds beside it; a generator
    /// that REWRITES the item must not go through it, or the item is emitted twice. Which of the
    /// two a pipeline is doing cannot be read off the generator's Ty(Output) - the framework sees a
    /// Tr(ToTokens) and no more - so it is declared.
    ///
    /// It sits on the GENERATOR role rather than on Attr(pipeline) because that is whose property
    /// it is: the generator is the thing that either includes the item in what it builds or does
    /// not. Attr(pipeline) itself is the worked example - it re-emits a STRIPPED module, so it
    /// declares `replace`
    pub enum Emission {
        Beside = "beside",
        Replace = "replace",
    }
}

impl Default for Emission {
    /// Adding beside is the safe default: a generator that forgets to declare anything cannot
    /// silently DELETE the author's item, only fail to rewrite it.
    fn default() -> Self {
        Self::Beside
    }
}

/// A whole `#[pipeline]` module.
pub(crate) struct PipelineExtraction<'ast> {
    /// Every item that declares a role. Items that declare none extract to nothing.
    pub(crate) components: Vec<Extracted<ComponentExtraction<'ast>, &'ast Item>>,
    /// Every `vocabulary!` the module declares, so the processor can find the one named by
    /// `helpers = ..` - see ID(pipeline-macro/helpers-are-read).
    pub(crate) vocabularies: Vec<Extracted<VocabularyExtraction<'ast>, &'ast Item>>,
}

/// What `#[pipeline]` is handed: its own arguments, and the module they were written on.
///
/// NOTE(#pipeline-macro/is-a-pipeline): V[Impl(Pipeline).for(PipelineWiring)], "The pipeline macro runs through Pipeline::run like any other"
/// This macro used to drive its own three stages by hand, and
/// ID(pipeline/subject-equals-source-breaks-attributes) recorded why: Tr(Pipeline) binds
/// `Generator::Subject = Validate::Source`, and an attribute macro has TWO inputs, so no Source
/// could carry both. That diagnosis was wrong about the cause. The binding was never the obstacle -
/// `Source` being a SINGLE NODE was. S(Attributed) is a node carrying both, so the binding holds
/// unchanged and the special case disappears.
///
/// The macro is now its own worked example: if the pipeline macro can be written as a pipeline,
/// an author's attribute macro can be.
pub(crate) type PipelineSource<'ast> = Attributed<'ast, PipelineArgs, ItemMod>;

impl<'ast> Validate<'ast> for PipelineExtraction<'ast> {
    type Source = PipelineSource<'ast>;
    /// The module's CONTENT. A module with no body has nothing to wire.
    type Valid = &'ast [Item];

    fn validate(input: Self::Source) -> Result<Self::Valid, Reason> {
        let module = input.item();
        match &module.content {
            Some((_, items)) => Ok(items),
            None => Err(Reason::at(ReasonKind::WrongShape, &module.ident)),
        }
    }
}

impl<'ast> Extractor<'ast> for PipelineExtraction<'ast> {
    type Output = Extracted<Self, PipelineSource<'ast>>;

    fn extract_from(node: PipelineSource<'ast>) -> Self::Output {
        let extraction = match Self::validate(node) {
            Ok(items) => Extraction::value(Self {
                components: ComponentExtraction::extract_each(items.iter()),
                vocabularies: VocabularyExtraction::extract_each(items.iter()),
            }),
            Err(reason) => Extraction::failed(reason),
        };

        Extracted::new(extraction, node)
    }
}

impl<'ast> Assert for PipelineExtraction<'ast> {}

impl<'ast> Diagnose for PipelineExtraction<'ast> {
    fn diagnose(&self, out: &mut Vec<syn::Error>) {
        self.components.diagnose(out);
        self.vocabularies.diagnose(out);
    }
}

/// One item that declares a role.
pub(crate) struct ComponentExtraction<'ast> {
    /// EVERY role this item declares, not the first.
    ///
    /// NOTE(#pipeline-macro/one-type-many-roles): V[S(Component).P(roles).has(many)], "One type may carry several roles"
    /// A type is routinely its own processor - ID(pipeline/no-processor-is-the-extractor) is the
    /// rule, and StructExtraction is the example - so `#[extractor(..)]` and `#[processor(from =
    /// ..)]` on one item has to work. Reading only the first role found silently dropped the
    /// second, which made the commonest pipeline of all inexpressible
    pub(crate) roles: Vec<Role>,
    /// The item's own name - what a sibling's `from = ..` has to match.
    pub(crate) name: &'ast syn::Ident,
    /// The role attribute's arguments, CARRIED and not read. ID(no-parse) again: the processor
    /// asks them questions, this stage only says where they are.
    pub(crate) args: &'ast proc_macro2::TokenStream,
}

impl<'ast> Validate<'ast> for ComponentExtraction<'ast> {
    type Source = &'ast Item;
    type Valid = (Vec<Role>, &'ast syn::Ident, &'ast proc_macro2::TokenStream);

    fn validate(input: &'ast Item) -> Result<Self::Valid, Reason> {
        let (attrs, name) = match input {
            Item::Struct(item) => (&item.attrs, &item.ident),
            Item::Enum(item) => (&item.attrs, &item.ident),
            Item::Type(item) => (&item.attrs, &item.ident),
            // Not a nameable item - a `vocabulary!` invocation, a `use`, anything. It is NOT a
            // malformed component, it is not a component, so it is silent. Telling those two apart
            // is the whole of ID(heads-are-rustcs), and getting it wrong here made a pipeline
            // module complain about its own vocabulary.
            other => return Err(Reason::at(ReasonKind::UnknownKey, other)),
        };

        let mut roles = Vec::new();
        let mut args = None;

        for role in Role::ALL {
            if let Some(attr) = attrs.find_one(role.spelling()).map_err(to_reason)? {
                match &attr.meta {
                    syn::Meta::List(list) => args = args.or(Some(&list.tokens)),
                    other => return Err(Reason::at(ReasonKind::WrongShape, other)),
                }
                roles.push(*role);
            }
        }

        if let (false, Some(args)) = (roles.is_empty(), args) {
            return Ok((roles, name, args));
        }

        // Wears no role: not ours, and NOT a complaint. ID(heads-are-rustcs).
        Err(Reason::at(ReasonKind::UnknownKey, input))
    }
}

impl<'ast> Extractor<'ast> for ComponentExtraction<'ast> {
    type Output = Extracted<Self, &'ast Item>;

    fn extract_from(node: &'ast Item) -> Self::Output {
        let extraction = match Self::validate(node) {
            Ok((roles, name, args)) => Extraction::value(Self { roles, name, args }),
            // An item with no role is SILENT - a `vocabulary!` or a plain helper struct is not a
            // mistake. Only the shape complaints above are kept.
            Err(reason) if matches!(reason.kind, ReasonKind::UnknownKey) => Extraction::default(),
            Err(reason) => Extraction::failed(reason),
        };

        Extracted::new(extraction, node)
    }
}

impl<'ast> Assert for ComponentExtraction<'ast> {}

impl<'ast> Diagnose for ComponentExtraction<'ast> {
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

/// A `vocabulary!` declared in the module.
///
/// NOTE(#pipeline-macro/helpers-are-read): V[S(VocabularyExtraction).has(lifted literals)], "Helper spellings are lifted from tokens, never redeclared"
/// The helper spellings are LIFTED FROM TOKENS, never redeclared. A proc macro cannot resolve
/// types, so it can never ask `SyntaxHelper` what its variants are - but M(vocabulary) writes them
/// as string LITERALS, and those are right there in the module's token stream.
///
/// So `helpers = SyntaxHelper` names a vocabulary and this stage finds it by that name, textually,
/// within the module. One declaration, two consumers: the matcher the author already had, and the
/// `attributes(..)` list rustc needs. That is what closes ID(pipeline/helpers-are-syntactic) with
/// no second list and no assertion to keep in step.
///
/// The price, and it is the right one: the vocabulary must live INSIDE the pipeline module. Naming
/// one that is not there is an error this stage can raise precisely, which beats today's silence
pub(crate) struct VocabularyExtraction<'ast> {
    #[allow(dead_code, reason = "'ast is carried for the stage traits, not for a field")]
    pub(crate) _marker: std::marker::PhantomData<&'ast ()>,
    pub(crate) name: syn::Ident,
    /// Every accepted spelling, in declaration order.
    pub(crate) spellings: Vec<syn::LitStr>,
}

impl<'ast> Validate<'ast> for VocabularyExtraction<'ast> {
    type Source = &'ast Item;
    type Valid = &'ast syn::ItemMacro;

    fn validate(input: &'ast Item) -> Result<Self::Valid, Reason> {
        match input {
            Item::Macro(item) if item.mac.path.segments.last().is_some_and(|segment| {
                segment.ident == "vocabulary" || segment.ident == "names"
            }) =>
            {
                Ok(item)
            }
            other => Err(Reason::at(ReasonKind::UnknownKey, other)),
        }
    }
}

impl<'ast> Extractor<'ast> for VocabularyExtraction<'ast> {
    type Output = Extracted<Self, &'ast Item>;

    fn extract_from(node: &'ast Item) -> Self::Output {
        let extraction = match Self::validate(node) {
            Ok(item) => match read_vocabulary(item) {
                Ok((name, spellings)) => Extraction::value(Self {
                    name,
                    spellings,
                    _marker: std::marker::PhantomData,
                }),
                Err(error) => Extraction::failed(Reason::new(ReasonKind::Syntax(error))),
            },
            // Not a vocabulary. Silent, like a component with no role.
            Err(_) => Extraction::default(),
        };

        Extracted::new(extraction, node)
    }
}

impl<'ast> Assert for VocabularyExtraction<'ast> {}

impl<'ast> Diagnose for VocabularyExtraction<'ast> {
    fn diagnose(&self, _: &mut Vec<syn::Error>) {}
}

/// The enum's name, and every spelling literal inside it.
///
/// Reads the macro's TOKENS rather than expanding it - the invocation has not run and will not run
/// until after this macro is done, so its expansion is not available at any price.
fn read_vocabulary(item: &syn::ItemMacro) -> syn::Result<(syn::Ident, Vec<syn::LitStr>)> {
    use proc_macro2::TokenTree;

    let mut trees = item.mac.tokens.clone().into_iter().peekable();
    let mut name = None;

    // `.. enum NAME { .. }` - find the ident after `enum`.
    while let Some(tree) = trees.next() {
        if let TokenTree::Ident(ident) = &tree
            && ident == "enum"
        {
            if let Some(TokenTree::Ident(found)) = trees.peek() {
                name = Some(found.clone());
            }
            break;
        }
    }

    let name = name.ok_or_else(|| {
        syn::Error::new_spanned(item, "expected `enum <Name> { .. }` inside the vocabulary")
    })?;

    // NOTE(#pipeline-macro/spellings-follow-an-equals): V[F(read_vocabulary) != reads(doc)], "A spelling follows an equals sign, so doc text is never read"
    // A spelling is a literal in a `Variant = \"name\"` position, and finding it needs BOTH halves
    // of the shape below. Taking every literal inside every top-level group instead - which is what
    // this did - swallows the text of any doc comment written above the enum: rustc turns `///`
    // into `#[doc = \"..\"]` even inside a macro invocation, and that bracket is a top-level group
    // holding a top-level literal.
    //
    // The result was a vocabulary whose spellings included whole sentences, which then failed
    // ID(pipeline-macro/helpers-are-idents) - correctly, and pointing at the doc comment. A
    // DOCUMENTED vocabulary in a pipeline module simply did not work.
    let body = item
        .mac
        .tokens
        .clone()
        .into_iter()
        .find_map(|tree| match tree {
            TokenTree::Group(group)
                if group.delimiter() == proc_macro2::Delimiter::Brace =>
            {
                Some(group.stream())
            }
            _ => None,
        })
        .unwrap_or_default();

    let mut spellings = Vec::new();
    let mut assigned = false;
    for tree in body {
        match tree {
            // `=` introduces the canonical spelling, `|` each further one.
            TokenTree::Punct(punct) if punct.as_char() == '=' || punct.as_char() == '|' => {
                assigned = true;
            }
            TokenTree::Literal(literal) if assigned => {
                if let Ok(spelling) =
                    syn::parse2::<syn::LitStr>(TokenTree::Literal(literal).into())
                {
                    spellings.push(spelling);
                }
                assigned = false;
            }
            _ => assigned = false,
        }
    }

    Ok((name, spellings))
}

fn to_reason(error: syn::Error) -> Reason {
    Reason::new(ReasonKind::Syntax(error))
}

/// The module with its role attributes REMOVED.
///
/// NOTE(#pipeline-macro/roles-are-stripped): V[F(stripped).has(removes Role)], "Role markers are stripped from the re-emitted items"
/// `#[extractor(..)]` and friends are markers read by this macro and registered with rustc by
/// NOBODY - so leaving them on the re-emitted items is `cannot find attribute` at the author's
/// site. An attribute macro owns what it emits, and that includes removing the markers it consumed.
///
/// It is the same obligation ID(pipeline/helpers-are-syntactic) describes from the other end: an
/// attribute either gets registered or gets removed, and a marker this macro invented can only be
/// the second
pub(crate) fn stripped(module: &ItemMod) -> ItemMod {
    let mut module = module.clone();

    if let Some((_, items)) = &mut module.content {
        for item in items {
            let attrs = match item {
                Item::Struct(item) => Some(&mut item.attrs),
                Item::Enum(item) => Some(&mut item.attrs),
                Item::Type(item) => Some(&mut item.attrs),
                _ => None,
            };

            if let Some(attrs) = attrs {
                attrs.retain(|attr| {
                    !Role::ALL
                        .iter()
                        .any(|role| attr.path().is_ident(role.spelling()))
                });
            }
        }
    }

    module
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    fn module(source: &str) -> ItemMod {
        parse_str(source).expect("the module parses")
    }

    /// A stand-in for what `#[pipeline(..)]` itself was invoked with.
    ///
    /// These tests are about the MODULE, not the invocation, so every one of them uses the same
    /// arguments. It has to exist because the source is a pair now - see
    /// ID(pipeline-macro/is-a-pipeline).
    fn invocation() -> PipelineArgs {
        PipelineArgs {
            kind: super::super::generator::MacroKind::Derive,
            exported: parse_str("Thing").expect("an ident"),
            entry: true,
        }
    }

    fn read(item: &ItemMod) -> Extracted<PipelineExtraction<'_>, PipelineSource<'_>> {
        // The arguments are leaked rather than threaded through every call site: they outlive the
        // test either way and nothing here reads them.
        let args: &'static PipelineArgs = Box::leak(Box::new(invocation()));
        PipelineExtraction::extract_from(Attributed::new(args, item))
    }

    const WHOLE: &str = r#"
        mod field_names {
            #[extractor(source = DeriveInput, helpers = SyntaxHelper)]
            struct Struct;

            #[processor(from = Struct)]
            struct Processed;

            #[generator(from = Processed)]
            struct Block;

            vocabulary! {
                pub enum SyntaxHelper {
                    Shape = "shape",
                    Alias = "alias",
                }
            }

            /// Not a stage and not a vocabulary - must be SILENT, not a complaint.
            struct Bystander;
        }
    "#;

    #[test]
    fn every_roled_item_is_a_component() {
        let item = module(WHOLE);
        let extracted = read(&item);
        let value = extracted.value().expect("the module extracts");

        let roles: Vec<Role> = value
            .components
            .iter()
            .filter_map(|c| c.value())
            .flat_map(|v| v.roles.iter().copied())
            .collect();

        assert_eq!(roles, [Role::Extractor, Role::Processor, Role::Generator]);
    }

    #[test]
    fn a_component_carries_the_name_a_sibling_would_point_at() {
        let item = module(WHOLE);
        let extracted = read(&item);
        let value = extracted.value().unwrap();

        let names: Vec<String> = value
            .components
            .iter()
            .filter_map(|c| c.value().map(|v| v.name.to_string()))
            .collect();

        assert_eq!(names, ["Struct", "Processed", "Block"]);
    }

    #[test]
    fn a_documented_vocabulary_does_not_leak_its_prose_into_the_helpers() {
        // REGRESSION for ID(pipeline-macro/spellings-follow-an-equals). Every literal inside
        // every top-level group was taken as a spelling, and `///` is `#[doc = ".."]` by the time
        // a macro sees it - so a documented vocabulary registered its own prose as helper
        // attributes and then failed for not being idents.
        let item = module(
            r#"mod m {
                #[extractor(source = D, helpers = H)] struct S;
                #[processor(from = S)] struct P;
                #[generator(from = P)] struct G;

                vocabulary! {
                    /// Prose that must not become a spelling.
                    pub enum H {
                        Shape = "shape" | "renamed",
                    }
                }
            }"#,
        );
        let value = read(&item).into_extraction().value.expect("extracts");
        let vocab = value.vocabularies.iter().find_map(|v| v.value()).expect("one vocabulary");

        let found: Vec<String> = vocab.spellings.iter().map(|s| s.value()).collect();
        assert_eq!(found, ["shape", "renamed"], "prose leaked in: {found:?}");
    }

    #[test]
    fn the_helper_spellings_are_lifted_from_the_vocabulary() {
        // ID(pipeline-macro/helpers-are-read). No type is resolved - the literals are read
        // straight out of the macro invocation's tokens.
        let item = module(WHOLE);
        let extracted = read(&item);
        let value = extracted.value().unwrap();

        let vocab = value
            .vocabularies
            .iter()
            .filter_map(|v| v.value())
            .next()
            .expect("the vocabulary is found");

        assert_eq!(vocab.name, "SyntaxHelper");
        let spellings: Vec<String> = vocab.spellings.iter().map(|s| s.value()).collect();
        assert_eq!(spellings, ["shape", "alias"]);
    }

    #[test]
    fn an_item_with_no_role_is_silent() {
        // ID(heads-are-rustcs), one level up: a plain struct in a pipeline module is not a
        // mistake, so it produces no value AND no complaint.
        let item = module(WHOLE);
        let extracted = read(&item);

        assert!(
            extracted.render().is_empty(),
            "a bystander item was complained about",
        );
    }

    #[test]
    fn a_module_with_no_body_has_nothing_to_wire() {
        let item = module("mod elsewhere;");
        let extracted = read(&item);

        assert!(extracted.value().is_none());
        assert_eq!(extracted.reasons().len(), 1);
    }

    #[test]
    fn a_role_written_without_arguments_is_a_complaint() {
        // `#[processor]` bare cannot say where it comes from, and that IS ours to report.
        let item = module("mod m { #[processor] struct P; }");
        let extracted = read(&item);
        let value = extracted.value().expect("the module still extracts");

        assert_eq!(value.components.len(), 1);
        assert!(value.components[0].value().is_none());
        assert_eq!(value.components[0].reasons().len(), 1);
    }

    #[test]
    fn one_type_may_wear_several_roles() {
        // ID(pipeline-macro/one-type-many-roles). The commonest pipeline of all - nothing to
        // process, so the extractor is named twice - was inexpressible while only the first role
        // was read.
        let item = module(
            r#"mod m {
                #[extractor(source = D)]
                #[processor(from = S)]
                struct S;
                #[generator(from = S)] struct G;
            }"#,
        );
        let extracted = read(&item);
        let value = extracted.value().expect("extracts");

        let first = value.components[0].value().expect("S is a component");
        assert_eq!(first.roles, [Role::Extractor, Role::Processor]);
    }

    #[test]
    fn the_re_emitted_module_has_no_role_attributes_left() {
        // ID(pipeline-macro/roles-are-stripped). Nothing registers `#[extractor]`, so leaving
        // it on is `cannot find attribute` at the author's site.
        let item = module(WHOLE);
        let out = quote::ToTokens::to_token_stream(&stripped(&item)).to_string();

        assert!(!out.contains("extractor"), "{out}");
        assert!(!out.contains("# [processor"), "{out}");
        assert!(out.contains("struct Struct"), "the items themselves must survive: {out}");
    }
}
