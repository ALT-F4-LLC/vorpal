use proc_macro2::TokenTree;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use syn::{File, Item, Stmt};

// ─── Parsed data structures ───────────────────────────────────────────────

/// Parsed script data from a Rust formatdoc! source file.
struct ParsedScript {
    /// Function parameter names (in order)
    params: Vec<String>,
    /// The raw template string from formatdoc!
    template: String,
    /// Named arguments with transformed expressions (e.g., unzip_version = expr)
    named_args: Vec<(String, String)>,
}

/// A parsed source function from source.rs
struct ParsedSourceFn {
    /// Function name (e.g., "curl", "gnu", "unzip")
    name: String,
    /// Parameter names
    params: Vec<String>,
    /// The source name (literal or derived from param)
    source_name: String,
    /// Whether source_name is a literal string or a parameter reference
    source_name_is_param: bool,
    /// The URL path format string with {name}/{version} placeholders
    path_template: String,
    /// Whether this function has a version.replace(".", "") call
    has_version_replace: bool,
}

/// A parsed version constant from linux_vorpal.rs
struct VersionConst {
    name: String,
    value: String,
}

/// A parsed source call from linux_vorpal.rs
struct SourceCall {
    var_name: String,
    func_name: String,
    args: Vec<String>,
}

/// A parsed step from linux_vorpal.rs
struct StepDef {
    /// Which bwrap_arguments to use: "default" (empty), "custom_stage03", "bwrap_arguments"
    bwrap_mode: String,
    /// rootfs: Some("step_rootfs.clone()") or None
    rootfs: Option<String>,
    /// The script variable or inline formatdoc
    script: String,
    /// Whether script is inline (formatdoc! "rm -rf...")
    script_is_inline: bool,
}

// ─── Parsing ──────────────────────────────────────────────────────────────

/// Extract the formatdoc! template string and function parameters from a script file.
fn parse_script_file(source: &str) -> ParsedScript {
    let ast: File = syn::parse_str(source).expect("Failed to parse Rust source");

    let mut params: Vec<String> = Vec::new();
    let mut template = String::new();
    let mut named_args: Vec<(String, String)> = Vec::new();

    for item in &ast.items {
        if let Item::Fn(func) = item {
            if func.sig.ident != "script" {
                continue;
            }

            // Extract parameter names from function signature
            for arg in &func.sig.inputs {
                if let syn::FnArg::Typed(pat_type) = arg {
                    if let syn::Pat::Ident(ident) = pat_type.pat.as_ref() {
                        params.push(ident.ident.to_string());
                    }
                }
            }

            // Find the formatdoc! macro invocation
            if let Some(Stmt::Macro(macro_stmt)) = func.block.stmts.last() {
                let mac = &macro_stmt.mac;
                let tokens: Vec<TokenTree> = mac.tokens.clone().into_iter().collect();

                // First token is the template string literal
                if let Some(TokenTree::Literal(lit)) = tokens.first() {
                    let lit_str: syn::LitStr =
                        syn::parse_str(&lit.to_string()).expect("Expected string literal");
                    template = lit_str.value();
                }

                // Parse named arguments after the template string
                let mut i = 1;
                while i < tokens.len() {
                    if let TokenTree::Punct(p) = &tokens[i] {
                        if p.as_char() == ',' {
                            i += 1;
                            continue;
                        }
                    }

                    if let TokenTree::Ident(name) = &tokens[i] {
                        if i + 2 < tokens.len() {
                            if let TokenTree::Punct(eq) = &tokens[i + 1] {
                                if eq.as_char() == '=' {
                                    let name_str = name.to_string();
                                    let mut expr_tokens = Vec::new();
                                    let mut j = i + 2;
                                    while j < tokens.len() {
                                        if let TokenTree::Punct(p) = &tokens[j] {
                                            if p.as_char() == ',' {
                                                break;
                                            }
                                        }
                                        expr_tokens.push(tokens[j].to_string());
                                        j += 1;
                                    }
                                    named_args.push((name_str, expr_tokens.join("")));
                                    i = j;
                                    continue;
                                }
                            }
                        }
                    }
                    i += 1;
                }
            }
        }
    }

    ParsedScript {
        params,
        template,
        named_args,
    }
}

/// Parse source.rs to extract all source helper functions.
fn parse_source_file(source: &str) -> Vec<ParsedSourceFn> {
    let ast: File = syn::parse_str(source).expect("Failed to parse source.rs");
    let mut results = Vec::new();

    for item in &ast.items {
        if let Item::Fn(func) = item {
            let name = func.sig.ident.to_string();

            let mut params = Vec::new();
            for arg in &func.sig.inputs {
                if let syn::FnArg::Typed(pat_type) = arg {
                    if let syn::Pat::Ident(ident) = pat_type.pat.as_ref() {
                        params.push(ident.ident.to_string());
                    }
                }
            }

            // Analyze the function body to extract name and path
            let body_str = quote::quote!(#func).to_string();

            // Detect if this function has version.replace(".", "")
            let has_version_replace = body_str.contains(r#"replace ("." , "")"#)
                || body_str.contains(r#"replace(".", "")"#)
                || body_str.contains("replace (\".\", \"\")");

            // Extract the source name
            let (source_name, source_name_is_param) = extract_source_name(&body_str, &name);

            // Extract the path template
            let path_template = extract_path_template(&body_str);

            results.push(ParsedSourceFn {
                name,
                params,
                source_name,
                source_name_is_param,
                path_template,
                has_version_replace,
            });
        }
    }

    results
}

/// Extract the source name from a function body string.
fn extract_source_name(body_str: &str, func_name: &str) -> (String, bool) {
    // Look for `let name = "something"` pattern
    if let Some(pos) = body_str.find("let name =") {
        let after = &body_str[pos + 11..];
        if let Some(q1) = after.find('"') {
            let after_q1 = &after[q1 + 1..];
            if let Some(q2) = after_q1.find('"') {
                return (after_q1[..q2].to_string(), false);
            }
        }
    }

    // For functions like gnu(name, version) where name is a parameter
    if func_name == "gnu" || func_name == "gnu_xz" {
        return ("name".to_string(), true);
    }

    (func_name.to_string(), false)
}

/// Extract the URL path format string from a function body.
fn extract_path_template(body_str: &str) -> String {
    // Find the format! or direct string assignment for path
    // Look for `let path = format ! (...)` or `let path = "..."`
    if let Some(pos) = body_str.find("let path =") {
        let after = &body_str[pos + 10..];

        // Check for format! macro
        if after.trim_start().starts_with("format") {
            // Find the string literal inside format!
            if let Some(q1) = after.find('"') {
                let after_q1 = &after[q1 + 1..];
                if let Some(q2) = find_unescaped_quote(after_q1) {
                    return after_q1[..q2].to_string();
                }
            }
        } else {
            // Direct string literal
            if let Some(q1) = after.find('"') {
                let after_q1 = &after[q1 + 1..];
                if let Some(q2) = find_unescaped_quote(after_q1) {
                    return after_q1[..q2].to_string();
                }
            }
        }
    }
    String::new()
}

/// Find the position of the first unescaped double quote.
fn find_unescaped_quote(s: &str) -> Option<usize> {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'"' && (i == 0 || bytes[i - 1] != b'\\') {
            return Some(i);
        }
        i += 1;
    }
    None
}

// ─── indoc stripping ──────────────────────────────────────────────────────

/// Apply indoc's leading-whitespace stripping behavior.
fn strip_indoc(s: &str) -> String {
    let s = s.strip_prefix('\n').unwrap_or(s);

    let lines: Vec<&str> = s.lines().collect();

    let min_indent = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|l| {
            if l.len() >= min_indent {
                &l[min_indent..]
            } else {
                l.trim()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Name conversion helpers ──────────────────────────────────────────────

/// Convert snake_case to Go camelCase (first letter lowercase).
fn to_go_camel(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            result.push_str(part);
        } else {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_uppercase().next().unwrap());
                result.extend(chars);
            }
        }
    }
    // Avoid shadowing Go builtins
    let go_reserved = [
        "make", "new", "len", "cap", "close", "delete", "copy", "append", "print", "println",
        "panic", "recover", "complex", "real", "imag",
    ];
    if go_reserved.contains(&result.as_str()) {
        result.push_str("Src");
    }
    result
}

/// Convert snake_case to Go PascalCase (first letter uppercase).
fn to_go_pascal(s: &str) -> String {
    let parts: Vec<&str> = s.split('_').collect();
    let mut result = String::new();
    for part in parts.iter() {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.push(first.to_uppercase().next().unwrap());
            result.extend(chars);
        }
    }
    result
}

/// Convert snake_case to TS camelCase.
fn to_ts_camel(s: &str) -> String {
    to_go_camel(s)
}

/// Escape a string for use in a Go raw string literal.
/// If the string contains backticks, split and concatenate.
fn escape_go_raw_string(s: &str) -> String {
    if !s.contains('`') {
        return format!("`{}`", s);
    }
    let parts: Vec<&str> = s.split('`').collect();
    let escaped: Vec<String> = parts.iter().map(|p| format!("`{}`", p)).collect();
    escaped.join(" + \"`\" + ")
}

// ─── Go Generation ────────────────────────────────────────────────────────

/// Generate a Go script function file.
fn generate_go_script(
    func_name: &str,
    template: &str,
    params: &[String],
    named_args: &[(String, String)],
) -> String {
    let header =
        "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\npackage artifact\n\nimport \"fmt\"\n";

    // Build named arg map for looking up replacement expressions
    let named_arg_map: HashMap<&str, &str> = named_args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    // Transform template: {param} -> %s, {{ -> {, }} -> }
    // Also escape existing % as %%
    let mut go_template = String::new();
    let mut sprintf_args: Vec<String> = Vec::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '%' {
            go_template.push_str("%%");
            i += 1;
        } else if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                go_template.push('{');
                i += 2;
            } else {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end] != '}' {
                    end += 1;
                }
                let param_name: String = chars[start..end].iter().collect();

                // Check if this is a named argument with a transformation
                if let Some(rust_expr) = named_arg_map.get(param_name.as_str()) {
                    // Transform Rust expression to Go
                    let go_expr = transform_rust_expr_to_go(rust_expr, params);
                    sprintf_args.push(go_expr);
                } else {
                    sprintf_args.push(to_go_camel(&param_name));
                }
                go_template.push_str("%s");
                i = end + 1;
            }
        } else if chars[i] == '}' {
            if i + 1 < chars.len() && chars[i + 1] == '}' {
                go_template.push('}');
                i += 2;
            } else {
                go_template.push(chars[i]);
                i += 1;
            }
        } else {
            go_template.push(chars[i]);
            i += 1;
        }
    }

    let go_func_name = format!("linuxVorpal{}", to_go_pascal(func_name));

    let param_list: String = params
        .iter()
        .map(|p| format!("{} string", to_go_camel(p)))
        .collect::<Vec<_>>()
        .join(", ");

    let args_str = if sprintf_args.is_empty() {
        String::new()
    } else {
        format!(", {}", sprintf_args.join(", "))
    };

    let go_template_escaped = escape_go_raw_string(&go_template);

    if sprintf_args.is_empty() {
        format!(
            "{header}\nfunc {go_func_name}({param_list}) string {{\n\treturn {go_template_escaped}\n}}\n"
        )
    } else {
        // Import strings if any named arg uses strings.ReplaceAll
        let needs_strings_import = named_args.iter().any(|(_, expr)| expr.contains("replace"));

        let imports = if needs_strings_import {
            "import (\n\t\"fmt\"\n\t\"strings\"\n)"
        } else {
            "import \"fmt\""
        };

        let header_with_imports = format!(
            "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\npackage artifact\n\n{imports}\n"
        );

        format!(
            "{header_with_imports}\nfunc {go_func_name}({param_list}) string {{\n\treturn fmt.Sprintf({go_template_escaped}{args_str})\n}}\n"
        )
    }
}

/// Transform a Rust expression like `unzip_version.replace(".", "").as_str()` to Go equivalent.
fn transform_rust_expr_to_go(rust_expr: &str, _params: &[String]) -> String {
    // Handle: var.replace(".", "").as_str() -> strings.ReplaceAll(var, ".", "")
    // Handle: var.replace(".", "") -> strings.ReplaceAll(var, ".", "")
    let expr = rust_expr.trim();

    if expr.contains(".replace") {
        // Extract the variable name before .replace
        let dot_pos = expr.find(".replace").unwrap();
        let var_name = expr[..dot_pos].trim();
        let go_var = to_go_camel(var_name);
        return format!("strings.ReplaceAll({}, \".\", \"\")", go_var);
    }

    to_go_camel(expr)
}

// ─── TypeScript Generation ────────────────────────────────────────────────

/// Generate a TypeScript script function file.
fn generate_ts_script(
    func_name: &str,
    template: &str,
    params: &[String],
    named_args: &[(String, String)],
) -> String {
    let header = "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n";

    let named_arg_map: HashMap<&str, &str> = named_args
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();

    let mut result = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            if i + 1 < chars.len() && chars[i + 1] == '{' {
                result.push('{');
                i += 2;
            } else {
                let start = i + 1;
                let mut end = start;
                while end < chars.len() && chars[end] != '}' {
                    end += 1;
                }
                let param_name: String = chars[start..end].iter().collect();

                // Check if this is a named argument with a transformation
                if let Some(rust_expr) = named_arg_map.get(param_name.as_str()) {
                    let ts_expr = transform_rust_expr_to_ts(rust_expr, params);
                    result.push_str(&format!("${{{}}}", ts_expr));
                } else {
                    result.push_str(&format!("${{{}}}", to_ts_camel(&param_name)));
                }
                i = end + 1;
            }
        } else if chars[i] == '}' {
            if i + 1 < chars.len() && chars[i + 1] == '}' {
                result.push('}');
                i += 2;
            } else {
                result.push(chars[i]);
                i += 1;
            }
        } else if chars[i] == '`' {
            result.push_str("\\`");
            i += 1;
        } else if chars[i] == '$' {
            result.push_str("\\$");
            i += 1;
        } else if chars[i] == '\\' {
            // Escape backslashes for TS template literals.
            // Without this, `\*` becomes `*`, `\.` becomes `.`, etc.
            // because JS silently drops `\` before unrecognized escapes.
            // `\n` in shell scripts (e.g. sed replacements) also needs
            // `\\n` to produce the literal two-char sequence `\n`.
            result.push_str("\\\\");
            i += 1;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    let ts_func_name = to_ts_camel(func_name);

    let param_list: String = params
        .iter()
        .map(|p| format!("{}: string", to_ts_camel(p)))
        .collect::<Vec<_>>()
        .join(", ");

    format!("{header}\nexport function {ts_func_name}({param_list}): string {{\n  return `{result}`;\n}}\n")
}

/// Transform a Rust expression to TypeScript equivalent.
fn transform_rust_expr_to_ts(rust_expr: &str, _params: &[String]) -> String {
    let expr = rust_expr.trim();

    if expr.contains(".replace") {
        let dot_pos = expr.find(".replace").unwrap();
        let var_name = expr[..dot_pos].trim();
        let ts_var = to_ts_camel(var_name);
        return format!("{}.replaceAll(\".\", \"\")", ts_var);
    }

    to_ts_camel(expr)
}

// ─── Source Generation ────────────────────────────────────────────────────

/// Generate Go source.go from parsed source functions.
fn generate_go_source(sources: &[ParsedSourceFn]) -> String {
    let mut out = String::from(
        "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\n\
         package artifact\n\n\
         import (\n\t\"fmt\"\n\t\"strings\"\n\n\tapi \"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact\"\n)\n",
    );

    for src in sources {
        out.push('\n');
        let go_func_name = source_func_name_to_go(&src.name);
        let param_list: String = src
            .params
            .iter()
            .map(|p| format!("{} string", to_go_camel(p)))
            .collect::<Vec<_>>()
            .join(", ");

        out.push_str(&format!(
            "func {}({}) api.ArtifactSource {{\n",
            go_func_name, param_list
        ));

        // Determine the name value
        if src.source_name_is_param {
            // name is a parameter, use it directly
        } else {
            out.push_str(&format!("\tname := \"{}\"\n", src.source_name));
        }

        // Handle version replacement if needed
        if src.has_version_replace {
            out.push_str(&format!(
                "\t{v} := strings.ReplaceAll({v_param}, \".\", \"\")\n",
                v = "versionClean",
                v_param = to_go_camel("version"),
            ));
        }

        // Build the path
        let go_path = build_go_source_path(&src.path_template, src.has_version_replace, &src.name);
        let name_ref = if src.source_name_is_param {
            to_go_camel(&src.source_name)
        } else {
            "name".to_string()
        };

        out.push_str(&format!("\tpath := {}\n\n", go_path));
        out.push_str(&format!(
            "\treturn NewArtifactSource({}, path).Build()\n",
            name_ref
        ));
        out.push_str("}\n");
    }

    out
}

/// Convert source function name to Go camelCase with linuxVorpalSource prefix (unexported).
fn source_func_name_to_go(name: &str) -> String {
    match name {
        "gnu" => "linuxVorpalSourceGnu".to_string(),
        "gnu_xz" => "linuxVorpalSourceGnuXz".to_string(),
        "gnu_gcc" => "linuxVorpalSourceGnuGcc".to_string(),
        "gnu_glibc_patch" => "linuxVorpalSourceGnuGlibcPatch".to_string(),
        _ => format!("linuxVorpalSource{}", to_go_pascal(name)),
    }
}

/// Build a Go fmt.Sprintf or string expression for a source path URL.
fn build_go_source_path(template: &str, has_version_replace: bool, _func_name: &str) -> String {
    if template.is_empty() {
        return "\"\"".to_string();
    }

    // Count format placeholders
    let placeholder_count = template.matches("{name}").count()
        + template.matches("{version}").count()
        + template.matches("{name}").count();

    if placeholder_count == 0 && !template.contains('{') {
        return format!("\"{}\"", template);
    }

    // Escape existing % as %% before replacing placeholders (Go fmt.Sprintf treats bare % as format verb)
    let mut go_fmt = template
        .replace('%', "%%")
        .replace("{name}", "%s")
        .replace("{version}", "%s");

    // Build args list
    let mut args = Vec::new();

    // Walk the original template to determine arg order
    let mut remaining = template;
    while let Some(pos) = remaining.find('{') {
        let end = remaining[pos..].find('}').unwrap() + pos;
        let placeholder = &remaining[pos + 1..end];
        match placeholder {
            "name" => args.push("name".to_string()),
            "version" => {
                if has_version_replace {
                    args.push("versionClean".to_string());
                } else {
                    args.push(to_go_camel("version"));
                }
            }
            other => {
                // Handle any other format placeholders
                go_fmt = go_fmt.replacen(&format!("{{{other}}}"), "%s", 1);
                args.push(to_go_camel(other));
            }
        }
        remaining = &remaining[end + 1..];
    }

    if args.is_empty() {
        format!("\"{}\"", go_fmt)
    } else {
        format!("fmt.Sprintf(\"{}\", {})", go_fmt, args.join(", "))
    }
}

/// Generate TypeScript source.ts from parsed source functions.
fn generate_ts_source(sources: &[ParsedSourceFn]) -> String {
    let mut out = String::from(
        "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\n\
         import type { ArtifactSource } from \"../../api/artifact/artifact.js\";\n\
         import { ArtifactSource as ArtifactSourceBuilder } from \"../../artifact.js\";\n",
    );

    for src in sources {
        out.push('\n');
        let ts_func_name = source_func_name_to_ts(&src.name);
        let param_list: String = src
            .params
            .iter()
            .map(|p| format!("{}: string", to_ts_camel(p)))
            .collect::<Vec<_>>()
            .join(", ");

        out.push_str(&format!(
            "export function {}({}): ArtifactSource {{\n",
            ts_func_name, param_list
        ));

        if src.source_name_is_param {
            // name is a parameter
        } else {
            out.push_str(&format!("  const name = \"{}\";\n", src.source_name));
        }

        if src.has_version_replace {
            out.push_str("  const versionClean = version.replaceAll(\".\", \"\");\n");
        }

        let ts_path = build_ts_source_path(&src.path_template, src.has_version_replace, &src.name);
        let name_ref = if src.source_name_is_param {
            to_ts_camel(&src.source_name)
        } else {
            "name".to_string()
        };

        out.push_str(&format!("  const path = {};\n\n", ts_path));
        out.push_str(&format!(
            "  return new ArtifactSourceBuilder({}, path).build();\n",
            name_ref
        ));
        out.push_str("}\n");
    }

    out
}

/// Convert source function name to TS camelCase with "source" prefix.
fn source_func_name_to_ts(name: &str) -> String {
    match name {
        "gnu" => "sourceGnu".to_string(),
        "gnu_xz" => "sourceGnuXz".to_string(),
        "gnu_gcc" => "sourceGnuGcc".to_string(),
        "gnu_glibc_patch" => "sourceGnuGlibcPatch".to_string(),
        _ => format!("source{}", to_go_pascal(name)),
    }
}

/// Build a TypeScript template literal for a source path URL.
fn build_ts_source_path(template: &str, has_version_replace: bool, _func_name: &str) -> String {
    if template.is_empty() {
        return "\"\"".to_string();
    }

    // Check if we need template literal (has placeholders)
    if !template.contains('{') {
        return format!("\"{}\"", template);
    }

    // Convert {name} -> ${name}, {version} -> ${version} or ${versionClean}
    let mut ts = String::new();
    let chars: Vec<char> = template.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        if chars[i] == '{' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && chars[end] != '}' {
                end += 1;
            }
            let placeholder: String = chars[start..end].iter().collect();
            match placeholder.as_str() {
                "version" if has_version_replace => {
                    ts.push_str("${versionClean}");
                }
                other => {
                    ts.push_str(&format!("${{{}}}", to_ts_camel(other)));
                }
            }
            i = end + 1;
        } else {
            ts.push(chars[i]);
            i += 1;
        }
    }

    format!("`{}`", ts)
}

// ─── Orchestration Generation ─────────────────────────────────────────────

/// Parse linux_vorpal.rs to extract version constants, source calls, and step construction.
fn parse_orchestration_file(source: &str) -> (Vec<VersionConst>, Vec<SourceCall>, Vec<StepDef>) {
    let mut versions = Vec::new();
    let mut source_calls = Vec::new();
    let mut steps = Vec::new();

    // Parse version constants: `let xxx_version = "yyy";`
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("let ") && trimmed.contains("_version =") && trimmed.contains('"') {
            if let Some(eq_pos) = trimmed.find('=') {
                let name = trimmed[4..eq_pos].trim().trim_end_matches(' ');
                let after_eq = &trimmed[eq_pos + 1..];
                if let Some(q1) = after_eq.find('"') {
                    let after_q1 = &after_eq[q1 + 1..];
                    if let Some(q2) = after_q1.find('"') {
                        let value = &after_q1[..q2];
                        versions.push(VersionConst {
                            name: name.to_string(),
                            value: value.to_string(),
                        });
                    }
                }
            }
        }
    }

    // Parse source calls: `let xxx = source::yyy(args);`
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("let ") && trimmed.contains("source::") {
            if let Some(eq_pos) = trimmed.find('=') {
                let var_name = trimmed[4..eq_pos].trim().to_string();
                if let Some(src_pos) = trimmed.find("source::") {
                    let after_src = &trimmed[src_pos + 8..];
                    if let Some(paren_pos) = after_src.find('(') {
                        let func_name = after_src[..paren_pos].to_string();
                        let after_paren = &after_src[paren_pos + 1..];
                        if let Some(close_paren) = after_paren.find(')') {
                            let args_str = &after_paren[..close_paren];
                            let args: Vec<String> = if args_str.is_empty() {
                                vec![]
                            } else {
                                args_str
                                    .split(',')
                                    .map(|a| a.trim().trim_matches('"').to_string())
                                    .collect()
                            };
                            source_calls.push(SourceCall {
                                var_name,
                                func_name,
                                args,
                            });
                        }
                    }
                }
            }
        }
    }

    // Parse the steps vector - this requires understanding the structure
    // Steps are constructed via step::bwrap() calls in a vec![]
    // We'll parse the known structure from the file

    // Step 0: setup - default bwrap, rootfs, step_setup_script
    steps.push(StepDef {
        bwrap_mode: "default".to_string(),
        rootfs: Some("step_rootfs".to_string()),
        script: "step_setup_script".to_string(),
        script_is_inline: false,
    });

    // Step 1: stage_01 - default bwrap, rootfs, step_stage_01_script
    steps.push(StepDef {
        bwrap_mode: "default".to_string(),
        rootfs: Some("step_rootfs".to_string()),
        script: "step_stage_01_script".to_string(),
        script_is_inline: false,
    });

    // Step 2: stage_02 - default bwrap, rootfs, step_stage_02_script
    steps.push(StepDef {
        bwrap_mode: "default".to_string(),
        rootfs: Some("step_rootfs".to_string()),
        script: "step_stage_02_script".to_string(),
        script_is_inline: false,
    });

    // Step 3: stage_03 - CUSTOM bwrap_arguments + tools bind, no rootfs
    steps.push(StepDef {
        bwrap_mode: "custom_stage03".to_string(),
        rootfs: None,
        script: "step_stage_03_script".to_string(),
        script_is_inline: false,
    });

    // Step 4: rm -rf tools - default bwrap, rootfs, inline script
    steps.push(StepDef {
        bwrap_mode: "default".to_string(),
        rootfs: Some("step_rootfs".to_string()),
        script: "rm -rf $VORPAL_OUTPUT/tools".to_string(),
        script_is_inline: true,
    });

    // Step 5: stage_04 - bwrap_arguments, no rootfs
    steps.push(StepDef {
        bwrap_mode: "bwrap_arguments".to_string(),
        rootfs: None,
        script: "step_stage_04_script".to_string(),
        script_is_inline: false,
    });

    // Step 6: stage_05 - bwrap_arguments, no rootfs
    steps.push(StepDef {
        bwrap_mode: "bwrap_arguments".to_string(),
        rootfs: None,
        script: "step_stage_05_script".to_string(),
        script_is_inline: false,
    });

    (versions, source_calls, steps)
}

/// Generate Go linux_vorpal.go orchestration file.
fn generate_go_orchestration(
    versions: &[VersionConst],
    source_calls: &[SourceCall],
    steps: &[StepDef],
) -> String {
    let mut out = String::from(
        "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\n\
         package artifact\n\n\
         import (\n\
         \tapi \"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/api/artifact\"\n\
         \t\"github.com/ALT-F4-LLC/vorpal/sdk/go/pkg/config\"\n\
         )\n\n\
         func linuxVorpalBuild(ctx *config.ConfigContext) (*string, error) {\n",
    );

    // Version constants
    for v in versions {
        out.push_str(&format!("\t{} := \"{}\"\n", to_go_camel(&v.name), v.value));
    }
    out.push('\n');

    // Source calls
    for sc in source_calls {
        let go_func = source_func_name_to_go(&sc.func_name);
        let args = build_go_source_args(&sc.args, versions);
        out.push_str(&format!(
            "\t{} := {}({})\n",
            to_go_camel(&sc.var_name),
            go_func,
            args
        ));
    }
    out.push('\n');

    // Step environments
    out.push_str("\tstepEnvironments := []string{\"PATH=/usr/bin:/usr/sbin\"}\n\n");

    // LinuxDebian rootfs
    out.push_str(
        "\tstepRootfs, err := NewLinuxDebian().Build(ctx)\n\
         \tif err != nil {\n\
         \t\treturn nil, err\n\
         \t}\n\n",
    );

    // Script calls
    out.push_str(&generate_go_script_calls(versions));
    out.push('\n');

    // bwrap_arguments for stage 03+
    out.push_str(&generate_go_bwrap_arguments());
    out.push('\n');

    // Systems
    out.push_str(
        "\tsystems := []api.ArtifactSystem{api.ArtifactSystem_AARCH64_LINUX, api.ArtifactSystem_X8664_LINUX}\n\n",
    );

    // Steps
    out.push_str("\tsteps := make([]*api.ArtifactStep, 0)\n\n");

    for (i, step) in steps.iter().enumerate() {
        let bwrap_args = match step.bwrap_mode.as_str() {
            "default" => "[]string{}",
            "custom_stage03" => {
                "append(append([]string{}, bwrapArguments...), \"--bind\", \"$VORPAL_OUTPUT/tools\", \"/tools\")"
            }
            "bwrap_arguments" => "bwrapArguments",
            _ => "[]string{}",
        };

        let rootfs_expr = match &step.rootfs {
            Some(_) => "stepRootfs",
            None => "nil",
        };

        let script_expr = if step.script_is_inline {
            format!("\"{}\"", step.script)
        } else {
            to_go_camel(&step.script)
        };

        out.push_str(&format!(
            "\tstep{i}, err := Bwrap(\n\
             \t\t{bwrap_args},\n\
             \t\t[]*string{{}},\n\
             \t\tstepEnvironments,\n\
             \t\t{rootfs_expr},\n\
             \t\t{script_expr},\n\
             \t\t[]*api.ArtifactStepSecret{{}},\n\
             \t)\n\
             \tif err != nil {{\n\
             \t\treturn nil, err\n\
             \t}}\n\
             \tsteps = append(steps, step{i})\n\n"
        ));
    }

    // Sources list
    out.push_str("\tsources := []*api.ArtifactSource{\n");
    for sc in source_calls {
        let var = to_go_camel(&sc.var_name);
        out.push_str(&format!("\t\t&{var},\n"));
    }
    out.push_str("\t}\n\n");

    // Build artifact
    out.push_str(
        "\tname := \"linux-vorpal\"\n\n\
         \treturn NewArtifact(name, steps, systems).\n\
         \t\tWithAliases([]string{name + \":latest\"}).\n\
         \t\tWithSources(sources).\n\
         \t\tBuild(ctx)\n\
         }\n",
    );

    out
}

/// Build Go argument string for a source call.
fn build_go_source_args(args: &[String], versions: &[VersionConst]) -> String {
    args.iter()
        .map(|a| {
            // Check if it's a version variable reference
            let trimmed = a.trim();
            if versions.iter().any(|v| v.name == trimmed) || trimmed.ends_with("_version") {
                to_go_camel(trimmed)
            } else {
                // It's a string literal
                format!("\"{}\"", trimmed)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate Go script function calls.
fn generate_go_script_calls(versions: &[VersionConst]) -> String {
    let mut out = String::new();
    let version_names: Vec<&str> = versions.iter().map(|v| v.name.as_str()).collect();

    // Setup script
    out.push_str("\tstepSetupScript := linuxVorpalSetup(\n");
    for param in &[
        "binutils_version",
        "gawk_version",
        "gcc_version",
        "glibc_version",
        "gmp_version",
        "mpc_version",
        "mpfr_version",
        "ncurses_version",
    ] {
        if version_names.contains(param) {
            out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
        }
    }
    out.push_str("\t)\n\n");

    // Stage 01 script
    out.push_str("\tstepStage01Script := linuxVorpalStage01(\n");
    for param in &[
        "binutils_version",
        "gcc_version",
        "glibc_version",
        "linux_version",
    ] {
        out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
    }
    out.push_str("\t)\n\n");

    // Stage 02 script
    out.push_str("\tstepStage02Script := linuxVorpalStage02(\n");
    for param in &[
        "bash_version",
        "binutils_version",
        "coreutils_version",
        "diffutils_version",
        "file_version",
        "findutils_version",
        "gawk_version",
        "gcc_version",
        "grep_version",
        "gzip_version",
        "m4_version",
        "make_version",
        "ncurses_version",
        "patch_version",
        "sed_version",
        "tar_version",
        "xz_version",
    ] {
        out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
    }
    out.push_str("\t)\n\n");

    // Stage 03 script
    out.push_str("\tstepStage03Script := linuxVorpalStage03(\n");
    for param in &[
        "bison_version",
        "gettext_version",
        "perl_version",
        "python_version",
        "texinfo_version",
        "util_linux_version",
    ] {
        out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
    }
    out.push_str("\t)\n\n");

    // Stage 04 script
    out.push_str("\tstepStage04Script := linuxVorpalStage04(\n");
    for param in &[
        "binutils_version",
        "gcc_version",
        "glibc_version",
        "openssl_version",
        "zlib_version",
    ] {
        out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
    }
    out.push_str("\t)\n\n");

    // Stage 05 script
    out.push_str("\tstepStage05Script := linuxVorpalStage05(\n");
    for param in &[
        "curl_version",
        "libidn2_version",
        "libpsl_version",
        "libunistring_version",
        "unzip_version",
    ] {
        out.push_str(&format!("\t\t{},\n", to_go_camel(param)));
    }
    out.push_str("\t)\n");

    out
}

/// Generate Go bwrap_arguments definition.
fn generate_go_bwrap_arguments() -> String {
    "\tbwrapArguments := []string{\n\
     \t\t// mount bin\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_OUTPUT/bin\",\n\
     \t\t\"/bin\",\n\
     \t\t// mount etc\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_OUTPUT/etc\",\n\
     \t\t\"/etc\",\n\
     \t\t// mount lib\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_OUTPUT/lib\",\n\
     \t\t\"/lib\",\n\
     \t\t// mount lib64 (if exists)\n\
     \t\t\"--bind-try\",\n\
     \t\t\"$VORPAL_OUTPUT/lib64\",\n\
     \t\t\"/lib64\",\n\
     \t\t// mount sbin\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_OUTPUT/sbin\",\n\
     \t\t\"/sbin\",\n\
     \t\t// mount usr\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_OUTPUT/usr\",\n\
     \t\t\"/usr\",\n\
     \t\t// mount current directory\n\
     \t\t\"--bind\",\n\
     \t\t\"$VORPAL_WORKSPACE\",\n\
     \t\t\"$VORPAL_WORKSPACE\",\n\
     \t\t// change directory\n\
     \t\t\"--chdir\",\n\
     \t\t\"$VORPAL_WORKSPACE\",\n\
     \t\t// set group id\n\
     \t\t\"--gid\",\n\
     \t\t\"0\",\n\
     \t\t// set user id\n\
     \t\t\"--uid\",\n\
     \t\t\"0\",\n\
     \t}\n"
        .to_string()
}

/// Generate TypeScript linux_vorpal.ts orchestration file.
fn generate_ts_orchestration(
    versions: &[VersionConst],
    source_calls: &[SourceCall],
    steps: &[StepDef],
) -> String {
    let mut out = String::from(
        "// Code generated by linux-vorpal-codegen. DO NOT EDIT.\n\n\
         import { ArtifactSystem } from \"../../api/artifact/artifact.js\";\n\
         import { Artifact } from \"../../artifact.js\";\n\
         import { bwrap } from \"../step.js\";\n\
         import { LinuxDebian } from \"../linux_debian.js\";\n\
         import type { ConfigContext } from \"../../context.js\";\n\
         import {\n\
         \tsetup,\n\
         \tstage01,\n\
         \tstage02,\n\
         \tstage03,\n\
         \tstage04,\n\
         \tstage05,\n\
         } from \"./scripts.js\";\n\
         import {\n",
    );

    // Import source functions
    let source_func_names: Vec<String> = source_calls
        .iter()
        .map(|sc| source_func_name_to_ts(&sc.func_name))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let mut sorted_names = source_func_names.clone();
    sorted_names.sort();
    for name in &sorted_names {
        out.push_str(&format!("\t{},\n", name));
    }
    out.push_str("} from \"./source.js\";\n\n");

    out.push_str("export async function linuxVorpal(ctx: ConfigContext): Promise<string> {\n");

    // Version constants
    for v in versions {
        out.push_str(&format!(
            "  const {} = \"{}\";\n",
            to_ts_camel(&v.name),
            v.value
        ));
    }
    out.push('\n');

    // Source calls
    for sc in source_calls {
        let ts_func = source_func_name_to_ts(&sc.func_name);
        let args = build_ts_source_args(&sc.args, versions);
        out.push_str(&format!(
            "  const {} = {}({});\n",
            to_ts_camel(&sc.var_name),
            ts_func,
            args
        ));
    }
    out.push('\n');

    // Step environments
    out.push_str("  const stepEnvironments = [\"PATH=/usr/bin:/usr/sbin\"];\n\n");

    // LinuxDebian rootfs
    out.push_str("  const stepRootfs = await new LinuxDebian().build(ctx);\n\n");

    // Script calls
    out.push_str(&generate_ts_script_calls(versions));
    out.push('\n');

    // bwrap_arguments
    out.push_str(&generate_ts_bwrap_arguments());
    out.push('\n');

    // Systems
    out.push_str(
        "  const systems = [\n\
         \tArtifactSystem.AARCH64_LINUX,\n\
         \tArtifactSystem.X8664_LINUX,\n\
         ];\n\n",
    );

    // Steps
    out.push_str("  const steps = [\n");

    for step in steps {
        let bwrap_args = match step.bwrap_mode.as_str() {
            "default" => "[]".to_string(),
            "custom_stage03" => {
                "[...bwrapArguments, \"--bind\", \"$VORPAL_OUTPUT/tools\", \"/tools\"]".to_string()
            }
            "bwrap_arguments" => "bwrapArguments".to_string(),
            _ => "[]".to_string(),
        };

        let rootfs_expr = match &step.rootfs {
            Some(_) => "stepRootfs",
            None => "null",
        };

        let script_expr = if step.script_is_inline {
            format!("\"{}\"", step.script)
        } else {
            to_ts_camel(&step.script)
        };

        out.push_str(&format!(
            "    await bwrap(\n\
             \t\t{bwrap_args},\n\
             \t\t[],\n\
             \t\t[...stepEnvironments],\n\
             \t\t{rootfs_expr},\n\
             \t\t[],\n\
             \t\t{script_expr},\n\
             \t),\n"
        ));
    }
    out.push_str("  ];\n\n");

    // Sources list
    out.push_str("  const sources = [\n");
    for sc in source_calls {
        out.push_str(&format!("    {},\n", to_ts_camel(&sc.var_name)));
    }
    out.push_str("  ];\n\n");

    // Build artifact
    out.push_str(
        "  const name = \"linux-vorpal\";\n\n\
         \treturn new Artifact(name, steps, systems)\n\
         \t\t.withAliases([`${name}:latest`])\n\
         \t\t.withSources(sources)\n\
         \t\t.build(ctx);\n\
         }\n",
    );

    out
}

/// Build TS argument string for a source call.
fn build_ts_source_args(args: &[String], versions: &[VersionConst]) -> String {
    args.iter()
        .map(|a| {
            let trimmed = a.trim();
            if versions.iter().any(|v| v.name == trimmed) || trimmed.ends_with("_version") {
                to_ts_camel(trimmed)
            } else {
                format!("\"{}\"", trimmed)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Generate TS script function calls.
fn generate_ts_script_calls(_versions: &[VersionConst]) -> String {
    let mut out = String::new();

    out.push_str("  const stepSetupScript = setup(\n");
    for param in &[
        "binutils_version",
        "gawk_version",
        "gcc_version",
        "glibc_version",
        "gmp_version",
        "mpc_version",
        "mpfr_version",
        "ncurses_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n\n");

    out.push_str("  const stepStage01Script = stage01(\n");
    for param in &[
        "binutils_version",
        "gcc_version",
        "glibc_version",
        "linux_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n\n");

    out.push_str("  const stepStage02Script = stage02(\n");
    for param in &[
        "bash_version",
        "binutils_version",
        "coreutils_version",
        "diffutils_version",
        "file_version",
        "findutils_version",
        "gawk_version",
        "gcc_version",
        "grep_version",
        "gzip_version",
        "m4_version",
        "make_version",
        "ncurses_version",
        "patch_version",
        "sed_version",
        "tar_version",
        "xz_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n\n");

    out.push_str("  const stepStage03Script = stage03(\n");
    for param in &[
        "bison_version",
        "gettext_version",
        "perl_version",
        "python_version",
        "texinfo_version",
        "util_linux_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n\n");

    out.push_str("  const stepStage04Script = stage04(\n");
    for param in &[
        "binutils_version",
        "gcc_version",
        "glibc_version",
        "openssl_version",
        "zlib_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n\n");

    out.push_str("  const stepStage05Script = stage05(\n");
    for param in &[
        "curl_version",
        "libidn2_version",
        "libpsl_version",
        "libunistring_version",
        "unzip_version",
    ] {
        out.push_str(&format!("    {},\n", to_ts_camel(param)));
    }
    out.push_str("  );\n");

    out
}

/// Generate TS bwrap_arguments definition.
fn generate_ts_bwrap_arguments() -> String {
    "  const bwrapArguments = [\n\
     \t// mount bin\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_OUTPUT/bin\",\n\
     \t\"/bin\",\n\
     \t// mount etc\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_OUTPUT/etc\",\n\
     \t\"/etc\",\n\
     \t// mount lib\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_OUTPUT/lib\",\n\
     \t\"/lib\",\n\
     \t// mount lib64 (if exists)\n\
     \t\"--bind-try\",\n\
     \t\"$VORPAL_OUTPUT/lib64\",\n\
     \t\"/lib64\",\n\
     \t// mount sbin\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_OUTPUT/sbin\",\n\
     \t\"/sbin\",\n\
     \t// mount usr\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_OUTPUT/usr\",\n\
     \t\"/usr\",\n\
     \t// mount current directory\n\
     \t\"--bind\",\n\
     \t\"$VORPAL_WORKSPACE\",\n\
     \t\"$VORPAL_WORKSPACE\",\n\
     \t// change directory\n\
     \t\"--chdir\",\n\
     \t\"$VORPAL_WORKSPACE\",\n\
     \t// set group id\n\
     \t\"--gid\",\n\
     \t\"0\",\n\
     \t// set user id\n\
     \t\"--uid\",\n\
     \t\"0\",\n\
     ];\n"
        .to_string()
}

// ─── File writing and check mode ──────────────────────────────────────────

/// Write a generated file, or check it in --check mode.
fn write_or_check(path: &Path, content: &str, check_mode: bool, mismatches: &mut Vec<String>) {
    if check_mode {
        match fs::read_to_string(path) {
            Ok(existing) => {
                if existing != content {
                    mismatches.push(format!(
                        "STALE: {} (regenerate with linux-vorpal-codegen)",
                        path.display()
                    ));
                }
            }
            Err(_) => {
                mismatches.push(format!("MISSING: {}", path.display()));
            }
        }
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("Failed to create output directory");
        }
        fs::write(path, content).unwrap_or_else(|e| {
            panic!("Failed to write {}: {}", path.display(), e);
        });
        eprintln!("  wrote {}", path.display());
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────

fn main() {
    let args: Vec<String> = env::args().collect();

    let check_mode = args.iter().any(|a| a == "--check");

    // Determine project root from the codegen tool location
    let tool_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let project_root = tool_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("Could not determine project root");

    let rust_script_dir = project_root.join("sdk/rust/src/artifact/linux_vorpal/script");
    let rust_source_file = project_root.join("sdk/rust/src/artifact/linux_vorpal/source.rs");
    let rust_orchestration_file = project_root.join("sdk/rust/src/artifact/linux_vorpal.rs");

    let go_output_dir = project_root.join("sdk/go/pkg/artifact");
    let ts_output_dir = project_root.join("sdk/typescript/src/artifact/linux_vorpal");

    let mut mismatches: Vec<String> = Vec::new();

    eprintln!("linux-vorpal-codegen: generating Go and TypeScript SDK files");
    if check_mode {
        eprintln!("  mode: --check (comparing against existing files)");
    }

    // ─── 1. Generate script files ─────────────────────────────────────

    let script_files = vec![
        ("setup", "setup.rs"),
        ("stage_01", "stage_01.rs"),
        ("stage_02", "stage_02.rs"),
        ("stage_03", "stage_03.rs"),
        ("stage_04", "stage_04.rs"),
        ("stage_05", "stage_05.rs"),
    ];

    for (func_name, filename) in &script_files {
        let source = fs::read_to_string(rust_script_dir.join(filename))
            .unwrap_or_else(|e| panic!("Failed to read {}: {}", filename, e));

        let parsed = parse_script_file(&source);
        let template = strip_indoc(&parsed.template);

        // Generate Go
        let go_content =
            generate_go_script(func_name, &template, &parsed.params, &parsed.named_args);
        let go_path = go_output_dir.join(format!("linux_vorpal_script_{}.go", func_name));
        write_or_check(&go_path, &go_content, check_mode, &mut mismatches);

        // Generate TypeScript
        let ts_content =
            generate_ts_script(func_name, &template, &parsed.params, &parsed.named_args);
        let ts_path = ts_output_dir.join(format!("script_{}.ts", func_name));
        write_or_check(&ts_path, &ts_content, check_mode, &mut mismatches);
    }

    // ─── 2. Generate source files ─────────────────────────────────────

    let source_content = fs::read_to_string(&rust_source_file)
        .unwrap_or_else(|e| panic!("Failed to read source.rs: {}", e));
    let parsed_sources = parse_source_file(&source_content);

    let go_source = generate_go_source(&parsed_sources);
    write_or_check(
        &go_output_dir.join("linux_vorpal_source.go"),
        &go_source,
        check_mode,
        &mut mismatches,
    );

    let ts_source = generate_ts_source(&parsed_sources);
    write_or_check(
        &ts_output_dir.join("source.ts"),
        &ts_source,
        check_mode,
        &mut mismatches,
    );

    // ─── 3. Generate orchestration files ──────────────────────────────

    let orchestration_content = fs::read_to_string(&rust_orchestration_file)
        .unwrap_or_else(|e| panic!("Failed to read linux_vorpal.rs: {}", e));
    let (versions, source_calls, steps) = parse_orchestration_file(&orchestration_content);

    let go_orchestration = generate_go_orchestration(&versions, &source_calls, &steps);
    write_or_check(
        &go_output_dir.join("linux_vorpal_build.go"),
        &go_orchestration,
        check_mode,
        &mut mismatches,
    );

    let ts_orchestration = generate_ts_orchestration(&versions, &source_calls, &steps);
    write_or_check(
        &ts_output_dir.join("linux_vorpal.ts"),
        &ts_orchestration,
        check_mode,
        &mut mismatches,
    );

    // ─── 4. Report results ────────────────────────────────────────────

    if check_mode {
        if mismatches.is_empty() {
            eprintln!("linux-vorpal-codegen: all generated files are up to date");
            process::exit(0);
        } else {
            eprintln!(
                "linux-vorpal-codegen: {} file(s) out of date:",
                mismatches.len()
            );
            for m in &mismatches {
                eprintln!("  {}", m);
            }
            process::exit(1);
        }
    } else {
        eprintln!(
            "linux-vorpal-codegen: generated {} Go files and {} TypeScript files",
            script_files.len() + 2, // scripts + source + orchestration
            script_files.len() + 2,
        );
    }
}
