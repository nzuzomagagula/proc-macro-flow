// @review [x]
pub mod extractor;
pub mod generator;
pub mod processor;

// TODO(#syntax/home):C[N(syntax)], "The syntax stage's public surface belongs here, beside the
//   other three: the shape traits (FromPath/FromMetaList/FromNameValue), the leaf trait FromExpr,
//   Reason, Extraction<T>, Node and the Diagnostics rendering hook. VERIFIED that a proc-macro
//   crate is refused outright when it declares a pub non-macro item, so these cannot live in
//   proc_macro_flow_derive - the grammar types an author writes must also be pub somewhere
//   ordinary, or they never reach cargo doc and a re-emitted path never resolves downstream.
//   Design and full task list: proc_macro_flow_derive/src/base/syntax/mod.rs"
