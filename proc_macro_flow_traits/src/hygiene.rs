// @review [ ]
// TODO[ ](#hygiene/scan-for-bare-paths): C[N(hygiene)], "Shipped FOUR times: a generated path
// naming syn, quote or proc_macro2 resolves at the CALL SITE, where the author has no reason to
// depend on any of them. Reviewing for it does not work - eighty-nine sites survived four rounds of
// it. A text scan does"
//! The one check that catches a class of bug this workspace has shipped four times.
//!
//! NOTE(#hygiene/generated-paths-are-checked): V[N(workspace).!emits(bare_path)], "Generated code
//! that names `::syn::`, `::quote::` or `::proc_macro2::` compiles perfectly HERE and fails in the
//! AUTHOR'S crate with `cannot find syn in the list of imported crates` - because those paths
//! resolve at the CALL SITE, where the author has no reason to depend on any of them.
//!
//! It has been found four times, each by accident and each the same way: a crate that depends on
//! the facade ALONE tries to use something. Every test in this workspace imports what it needs
//! directly, so none of them can see it - the facade itself carries `syn` as a dev-dependency, and
//! that single line hid the whole class.
//!
//! The rounds, so the shape is on record: Tr(Diagnose) in generated derive output; M(flag),
//! M(name_value) and M(variants); seven sites in the Attr(derive(Generator)) and
//! Attr(derive(Syntax)) emissions; then eighty-nine across the whole vocabulary suite. Reviewing
//! for it does not work. This does.
//!
//! WHAT IT CANNOT CATCH, stated so nobody trusts it further than it goes: it is a text scan, so a
//! path assembled at runtime or spelled through an alias passes. The real guarantee is the one
//! N(proc_macro_flow_demo) gives - a crate with no `syn` dependency that uses the macros for real."

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    /// The three crates whose generated code lands in somebody else's.
    fn sources() -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("the workspace root is this crate's parent")
            .to_path_buf();

        let mut out = Vec::new();
        for crate_dir in ["proc_macro_flow_traits", "proc_macro_flow_derive"] {
            walk(&root.join(crate_dir).join("src"), &mut out);
        }
        out
    }

    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    /// A line that is prose rather than code.
    ///
    /// The NOTEs in this workspace quote the very paths being banned - explaining the bug is not
    /// committing it - so a scan that cannot tell them apart would be unusable.
    fn is_prose(line: &str) -> bool {
        let trimmed = line.trim_start();
        trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("* ")
    }

    #[test]
    fn no_generated_code_names_a_crate_the_author_does_not_depend_on() {
        // `syn` is the one an author MIGHT have; `quote` and `proc_macro2` they almost never do.
        // All three are re-exported, so all three have a correct spelling and no excuse.
        const BANNED: [&str; 3] = ["::syn::", "::quote::", "::proc_macro2::"];

        // The two spellings that are FINE: `$crate::` from a macro_rules body, and the full path
        // from a `quote!`. Both resolve to this crate wherever they land.
        const ALLOWED: [&str; 5] = [
            "$crate::syn::",
            "$crate::quote::",
            "$crate::proc_macro2::",
            "proc_macro_flow_traits::",
            "crate::",
        ];

        let mut found = Vec::new();

        for path in sources() {
            // This file quotes every banned path in its own prose and in these very constants.
            if path.ends_with("hygiene.rs") {
                continue;
            }

            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };

            for (number, line) in text.lines().enumerate() {
                if is_prose(line) {
                    continue;
                }

                for banned in BANNED {
                    let mut at = 0;
                    while let Some(hit) = line[at..].find(banned) {
                        let start = at + hit;
                        let prefix = &line[..start];
                        if !ALLOWED.iter().any(|ok| {
                            prefix.ends_with(ok.trim_end_matches(banned.trim_start_matches("::")))
                                || line[..start + banned.len()].ends_with(ok)
                        }) && !ALLOWED
                            .iter()
                            .any(|ok| line[..start + banned.len()].contains(ok))
                        {
                            found.push(format!(
                                "{}:{}: {}",
                                path.display(),
                                number + 1,
                                line.trim()
                            ));
                        }
                        at = start + banned.len();
                    }
                }
            }
        }

        assert!(
            found.is_empty(),
            "generated code names a crate the author has no reason to depend on.\n\
             Write `$crate::syn::..` from a macro_rules body, or \
             `::proc_macro_flow_traits::syn::..` from a `quote!`.\n\n{}",
            found.join("\n")
        );
    }

    #[test]
    fn the_scan_can_tell_prose_from_code() {
        // The guard is worth nothing if it fires on the NOTEs that explain it, and worth less if
        // it cannot see a real one. Both directions, asserted.
        assert!(is_prose("// a NOTE mentioning ::syn::Error"));
        assert!(is_prose("    /// doc mentioning ::quote::ToTokens"));
        assert!(!is_prose("    type Error = ::syn::Error;"));
    }
}
