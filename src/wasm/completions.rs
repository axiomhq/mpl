//! Autocompletion and function info for `MPL` queries.
use std::{collections::HashMap, sync::LazyLock};

use pest::Parser as _;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{
    linker::{ArgType, FunctionId, FunctionTrait, Module},
    parser::{MPLParser, Rule},
    stdlib::STDLIB,
};

use super::Span;

#[derive(Clone, Serialize)]
pub(super) struct CompletionArg {
    pub(super) name: &'static str,
    #[serde(rename = "type")]
    pub(super) typ: ArgType,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ParamType {
    Dataset,
    Metric,
    Duration,
    String,
    Int,
    Float,
    Bool,
    Regex,
}

#[derive(Clone, Serialize)]
pub(super) struct ParamItem {
    pub(super) label: std::string::String,
    #[serde(rename = "type")]
    pub(super) typ: ParamType,
}

#[derive(Clone, Serialize)]
pub(super) struct KeywordItem {
    pub(super) label: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) apply: Option<&'static str>,
    pub(super) info: &'static str,
}

#[derive(Clone, Serialize)]
pub(super) struct FunctionItem {
    pub(super) label: String,
    pub(super) args: Vec<CompletionArg>,
    pub(super) info: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum CompletionResult {
    Keywords {
        #[serde(flatten)]
        span: Span,
        options: Vec<KeywordItem>,
    },
    AlignFunctions {
        #[serde(flatten)]
        span: Span,
        options: Vec<FunctionItem>,
    },
    MapFunctions {
        #[serde(flatten)]
        span: Span,
        options: Vec<FunctionItem>,
    },
    GroupFunctions {
        #[serde(flatten)]
        span: Span,
        options: Vec<FunctionItem>,
    },
    BucketFunctions {
        #[serde(flatten)]
        span: Span,
        options: Vec<FunctionItem>,
    },
    ComputeFunctions {
        #[serde(flatten)]
        span: Span,
        options: Vec<FunctionItem>,
    },
    Tag {
        #[serde(flatten)]
        span: Span,
        dataset: String,
        metric: String,
    },
    Dataset {
        #[serde(flatten)]
        span: Span,
    },
    Metric {
        #[serde(flatten)]
        span: Span,
        dataset: String,
    },
    Params {
        #[serde(flatten)]
        span: Span,
        options: Vec<ParamItem>,
    },
}

impl CompletionResult {
    fn retain_options(&mut self, f: impl Fn(&str) -> bool) {
        match self {
            Self::Keywords { options, .. } => options.retain(|item| f(item.label)),
            Self::AlignFunctions { options, .. }
            | Self::MapFunctions { options, .. }
            | Self::GroupFunctions { options, .. }
            | Self::BucketFunctions { options, .. }
            | Self::ComputeFunctions { options, .. } => options.retain(|item| f(&item.label)),
            Self::Params { options, .. } => options.retain(|item| f(&item.label)),
            Self::Tag { .. } | Self::Dataset { .. } | Self::Metric { .. } => {}
        }
    }

    #[cfg(test)]
    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Keywords { .. } => "keywords",
            Self::AlignFunctions { .. } => "align_functions",
            Self::MapFunctions { .. } => "map_functions",
            Self::GroupFunctions { .. } => "group_functions",
            Self::BucketFunctions { .. } => "bucket_functions",
            Self::ComputeFunctions { .. } => "compute_functions",
            Self::Tag { .. } => "tag",
            Self::Dataset { .. } => "dataset",
            Self::Metric { .. } => "metric",
            Self::Params { .. } => "params",
        }
    }

    #[cfg(test)]
    pub(super) fn option_labels(&self) -> Vec<&str> {
        match self {
            Self::Keywords { options, .. } => options.iter().map(|o| o.label).collect(),
            Self::AlignFunctions { options, .. }
            | Self::MapFunctions { options, .. }
            | Self::GroupFunctions { options, .. }
            | Self::BucketFunctions { options, .. }
            | Self::ComputeFunctions { options, .. } => {
                options.iter().map(|o| o.label.as_str()).collect()
            }
            Self::Params { options, .. } => options.iter().map(|o| o.label.as_str()).collect(),
            Self::Tag { .. } | Self::Dataset { .. } | Self::Metric { .. } => vec![],
        }
    }

    #[cfg(test)]
    pub(super) fn keyword_apply_texts(&self) -> Vec<Option<&str>> {
        match self {
            Self::Keywords { options, .. } => options.iter().map(|o| o.apply).collect(),
            _ => vec![],
        }
    }
}

/// Returns completion suggestions for the given cursor position.
#[must_use]
#[wasm_bindgen]
pub fn completions(query: &str, cursor_pos: usize) -> JsValue {
    let result = compute_completions(query, cursor_pos);
    super::to_js_value(&result)
}

// ── function info / stdlib querying ─────────────────────────────

/// Information about a single stdlib function, returned by `function_info`.
#[derive(Serialize)]
pub(super) struct FunctionInfo {
    pub(super) label: String,
    pub(super) args: Vec<CompletionArg>,
    pub(super) info: Option<String>,
}

/// Looks up a stdlib function by its qualified label (e.g. `"avg"` or
/// `"prom::rate"`) and returns its argument signature and documentation.
#[must_use]
#[wasm_bindgen]
pub fn function_info(label: &str) -> JsValue {
    let result = STDLIB.lookup_function(label);
    super::to_js_value(&result)
}

fn collect_args<F: FunctionTrait>(f: &F) -> Vec<CompletionArg> {
    f.args()
        .into_iter()
        .map(|a| CompletionArg {
            name: a.name,
            typ: a.typ,
        })
        .collect()
}

fn collect_functions_from_module<F: FunctionTrait>(
    fns: &HashMap<FunctionId, F>,
    prefix: Option<&str>,
) -> Vec<FunctionItem> {
    fns.iter()
        .map(|(id, f)| {
            let label = match prefix {
                Some(p) => format!("{p}::{}", id.0),
                None => id.0.clone(),
            };
            FunctionItem {
                label,
                args: collect_args(f),
                info: f.doc().to_string(),
            }
        })
        .collect()
}

impl Module {
    /// Iterates this module and all nested submodules, calling `f` with each
    /// module and its optional qualified prefix (e.g. `Some("prom")`).
    fn for_each_module(&self, mut f: impl FnMut(&Module, Option<&str>)) {
        Self::for_each_module_rec(self, None, &mut f);
    }

    fn for_each_module_rec(
        module: &Module,
        prefix: Option<&str>,
        f: &mut dyn FnMut(&Module, Option<&str>),
    ) {
        f(module, prefix);
        for sub in module.submodules.values() {
            let sub_prefix = match prefix {
                Some(p) => format!("{p}::{}", sub.name.0),
                None => sub.name.0.clone(),
            };
            Self::for_each_module_rec(sub, Some(&sub_prefix), f);
        }
    }

    /// Collects all function names (including qualified submodule names) from
    /// a specific category across this module tree.
    fn function_names<V>(
        &self,
        accessor: impl Fn(&Module) -> &HashMap<FunctionId, V>,
    ) -> Vec<String> {
        let mut names = Vec::new();
        self.for_each_module(|module, prefix| {
            for key in accessor(module).keys() {
                match prefix {
                    Some(p) => names.push(format!("{p}::{}", key.0)),
                    None => names.push(key.0.clone()),
                }
            }
        });
        names
    }

    /// Collects completion items for a specific function category across this
    /// module tree.
    fn completion_items<F: FunctionTrait>(
        &self,
        accessor: impl Fn(&Module) -> &HashMap<FunctionId, F>,
    ) -> Vec<FunctionItem> {
        let mut items = Vec::new();
        self.for_each_module(|module, prefix| {
            let fns = accessor(module);
            if !fns.is_empty() {
                items.extend(collect_functions_from_module(fns, prefix));
            }
        });
        items
    }

    /// Searches all function categories in this module for the given function id.
    fn function_info_by_id(&self, fn_id: &str) -> Option<FunctionInfo> {
        fn try_map<F: FunctionTrait>(
            fns: &HashMap<FunctionId, F>,
            fn_id: &str,
            label: &str,
        ) -> Option<FunctionInfo> {
            let f = fns.get(fn_id)?;
            Some(FunctionInfo {
                label: label.to_string(),
                args: collect_args(f),
                info: Some(f.doc().to_string()),
            })
        }

        None.or_else(|| try_map(&self.mapping_functions, fn_id, fn_id))
            .or_else(|| try_map(&self.align_functions, fn_id, fn_id))
            .or_else(|| try_map(&self.group_functions, fn_id, fn_id))
            .or_else(|| try_map(&self.compute_functions, fn_id, fn_id))
            .or_else(|| try_map(&self.bucket_functions, fn_id, fn_id))
    }

    /// Walks down the submodule tree following the `::` separated path segments.
    fn resolve_module_path(&self, path: &str) -> Option<&Module> {
        let mut current = self;
        for segment in path.split("::") {
            current = current.submodules.get(segment)?;
        }
        Some(current)
    }

    /// Searches all nested submodules for an unqualified function name,
    /// returning the result with a fully qualified label.
    fn lookup_unqualified(&self, fn_name: &str) -> Option<FunctionInfo> {
        for sub in self.submodules.values() {
            if let Some(mut info) = sub.function_info_by_id(fn_name) {
                info.label = format!("{}::{fn_name}", sub.name.0);
                return Some(info);
            }
            if let Some(mut info) = sub.lookup_unqualified(fn_name) {
                info.label = format!("{}::{}", sub.name.0, info.label);
                return Some(info);
            }
        }
        None
    }

    /// Looks up a function by qualified label (e.g. `"avg"` or `"prom::rate"`),
    /// searching this module tree.
    pub(super) fn lookup_function(&self, label: &str) -> Option<FunctionInfo> {
        if let Some((module_path, fn_name)) = label.rsplit_once("::") {
            let module = self.resolve_module_path(module_path)?;
            let mut info = module.function_info_by_id(fn_name)?;
            info.label = label.to_string();
            Some(info)
        } else {
            self.function_info_by_id(label)
                .or_else(|| self.lookup_unqualified(label))
        }
    }
}

// ── cached stdlib completion items ──────────────────────────────

static ALIGN_COMPLETIONS: LazyLock<Vec<FunctionItem>> =
    LazyLock::new(|| STDLIB.completion_items(|m| &m.align_functions));
static MAP_COMPLETIONS: LazyLock<Vec<FunctionItem>> =
    LazyLock::new(|| STDLIB.completion_items(|m| &m.mapping_functions));
static GROUP_COMPLETIONS: LazyLock<Vec<FunctionItem>> =
    LazyLock::new(|| STDLIB.completion_items(|m| &m.group_functions));
static BUCKET_COMPLETIONS: LazyLock<Vec<FunctionItem>> =
    LazyLock::new(|| STDLIB.completion_items(|m| &m.bucket_functions));
static COMPUTE_COMPLETIONS: LazyLock<Vec<FunctionItem>> =
    LazyLock::new(|| STDLIB.completion_items(|m| &m.compute_functions));

// Cached function name candidate lists for diagnostic fuzzy-matching.
pub(super) static MAP_FN_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| STDLIB.function_names(|m| &m.mapping_functions));
pub(super) static ALIGN_FN_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| STDLIB.function_names(|m| &m.align_functions));
pub(super) static BUCKET_FN_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| STDLIB.function_names(|m| &m.bucket_functions));
pub(super) static GROUP_FN_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| STDLIB.function_names(|m| &m.group_functions));
pub(super) static COMPUTE_FN_NAMES: LazyLock<Vec<String>> =
    LazyLock::new(|| STDLIB.function_names(|m| &m.compute_functions));

// ── query context & completion engine ───────────────────────────

/// Identifies which part of a query the cursor is in so completions can be
/// scoped to the relevant subquery text.
pub(super) enum QueryContext<'a> {
    /// Inside a simple query, or inside one of the subqueries within compute
    /// braces. The slice covers only the current subquery.
    Subquery(&'a str),
    /// After the closing `}` of a compute query, at the first pipe position
    /// (where the `compute_rule` must appear).
    ComputeRulePipe(&'a str),
    /// After the closing `}` of a compute query, at a subsequent pipe position
    /// (where regular `pipe_rule`s appear — no filter/where).
    ComputeTailPipe(&'a str),
}

/// Returns `true` when `(` at `pos` opens a compute query tuple rather than a
/// function call or filter grouping. A compute `(` is only preceded by
/// start-of-input, `;` (directive end), `(` (nested compute), or `,` (second
/// subquery). Any identifier character or backtick means a function call.
/// `//` line comments are skipped when scanning backwards.
fn is_compute_paren(bytes: &[u8], pos: usize) -> bool {
    let mut j = pos;
    loop {
        while j > 0 && bytes[j - 1].is_ascii_whitespace() {
            j -= 1;
        }
        if j == 0 {
            return true;
        }
        // If we landed inside a // line comment, skip back past it.
        let line_start = bytes[..j]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |p| p + 1);
        match find_line_comment(&bytes[line_start..j]) {
            Some(offset) => j = line_start + offset,
            None => break,
        }
    }
    matches!(bytes[j - 1], b';' | b'(' | b',')
}

/// Finds the byte offset of the first `//` on a line that is not inside a
/// string or backtick-escaped identifier.
fn find_line_comment(line: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i + 1 < line.len() {
        match line[i] {
            b'"' | b'`' => {
                let delim = line[i];
                i += 1;
                while i < line.len() && line[i] != delim {
                    if line[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                if i < line.len() {
                    i += 1;
                }
            }
            b'/' if line[i + 1] == b'/' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// Determines the query context for the text before the cursor.
///
/// Uses a stack to track brace nesting, producing a scoped text slice that
/// `suggest_for_context` and `extract_source_info` can operate on correctly
/// without needing brace-awareness themselves.
pub(super) fn locate_query_context(text: &str) -> QueryContext<'_> {
    let bytes = text.as_bytes();
    let len = bytes.len();

    // Stack of subquery start positions; base entry represents top-level.
    let mut stack: Vec<usize> = vec![0];
    let mut last_close_brace: Option<usize> = None;
    // Depth counter for non-compute parentheses (function calls, filter
    // grouping). While > 0, all nested parens and commas are ignored.
    let mut ignored_paren_depth: usize = 0;
    let mut i = 0;

    while i < len {
        if !skip_literal(bytes, len, &mut i) {
            match bytes[i] {
                b'(' => {
                    if ignored_paren_depth > 0 {
                        ignored_paren_depth += 1;
                    } else if is_compute_paren(bytes, i) {
                        stack.push(i + 1);
                    } else {
                        ignored_paren_depth += 1;
                    }
                }
                b')' => {
                    if ignored_paren_depth > 0 {
                        ignored_paren_depth -= 1;
                    } else if stack.len() > 1 {
                        stack.pop();
                        if stack.len() == 1 {
                            last_close_brace = Some(i);
                        }
                    }
                }
                b',' => {
                    if ignored_paren_depth == 0
                        && stack.len() > 1
                        && let Some(top) = stack.last_mut()
                    {
                        *top = i + 1;
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }

    // Inside braces — scope to the current subquery
    if stack.len() > 1 {
        let start = stack.last().copied().unwrap_or(0);
        return QueryContext::Subquery(&text[start..]);
    }

    // Outside braces — check if this is a compute query (we saw `{ ... }`)
    if let Some(brace_pos) = last_close_brace {
        let outer = &text[brace_pos + 1..];
        // Count escape-aware pipes in the outer text to distinguish the
        // compute_rule pipe (first) from subsequent pipe_rule pipes.
        let pipe_count = count_pipes(outer);
        return if pipe_count <= 1 {
            QueryContext::ComputeRulePipe(outer)
        } else {
            QueryContext::ComputeTailPipe(outer)
        };
    }

    // Simple query
    QueryContext::Subquery(text)
}

/// Counts escape-aware pipe characters in text.
fn count_pipes(text: &str) -> usize {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut count = 0;
    let mut i = 0;
    while i < len {
        if !skip_literal(bytes, len, &mut i) && bytes[i] == b'|' {
            count += 1;
        }
        i += 1;
    }
    count
}

pub(super) fn compute_completions(query: &str, cursor_pos: usize) -> Option<CompletionResult> {
    let cursor = cursor_pos.min(query.len());
    let (word_start, partial) = extract_partial_word(query, cursor);
    let before = &query[..word_start];
    let params = extract_declared_params(query);

    let span = Span::new(word_start, cursor);
    let mut result = match locate_query_context(before) {
        QueryContext::Subquery(text) => {
            suggest_for_context(text, span, FilterPolicy::Include, &params)
                .or_else(|| suggest_for_preamble(text, partial, span))
                .or_else(|| suggest_for_source(text, partial, span, &params))?
        }
        QueryContext::ComputeRulePipe(text) => suggest_for_compute_rule(text, span)?,
        QueryContext::ComputeTailPipe(text) => {
            suggest_for_context(text, span, FilterPolicy::Exclude, &params)?
        }
    };

    // For Tag completions where the user typed an opening backtick, advance
    // span.from past the backtick so the TS adapter can detect the backtick
    // context (doc.charAt(from - 1) === '`') and filter against bare tag names.
    if partial.starts_with('`')
        && let CompletionResult::Tag { ref mut span, .. } = result
    {
        span.from += 1;
    }

    // Strip a leading backtick from partial before filtering — the backtick is
    // a delimiter, not part of the identifier the user is typing.
    // For Params, the span may have been narrowed (e.g. to just the metric
    // fragment `$m` in `ds:$m`), so derive the filter text from the span.
    let filter_partial = if let CompletionResult::Params { ref span, .. } = result {
        &query[span.from..cursor]
    } else {
        partial.strip_prefix('`').unwrap_or(partial)
    };
    if !filter_partial.is_empty() {
        let lower = filter_partial.to_lowercase();
        result.retain_options(|label| label.to_lowercase().starts_with(&lower));
    }

    Some(result)
}

// ── text scanning utilities ─────────────────────────────────────

pub(super) fn extract_partial_word(text: &str, cursor: usize) -> (usize, &str) {
    let bytes = &text.as_bytes()[..cursor];
    let mut i = bytes.len();

    while i > 0 {
        match bytes[i - 1] {
            b'`' => {
                // Could be a closing backtick (matched pair) or an opening
                // backtick (user still typing an escaped ident).
                let backtick_pos = i - 1;
                i -= 1;
                let mut found_open = false;
                while i > 0 {
                    // Whitespace cannot appear inside a backtick identifier,
                    // so crossing it means this backtick is an unclosed opener
                    // for a new token, not the closer of a previous pair.
                    if bytes[i - 1].is_ascii_whitespace() {
                        break;
                    }
                    if bytes[i - 1] == b'`' && !is_char_escaped(bytes, i - 1) {
                        i -= 1;
                        found_open = true;
                        break;
                    }
                    i -= 1;
                }
                if !found_open {
                    // No matching opening backtick — the backtick we saw is
                    // the opening delimiter of an unclosed escaped ident.
                    // Continue scanning so preceding ident chars (e.g. `ds:`)
                    // are included.
                    i = backtick_pos;
                }
            }
            c if c.is_ascii_alphanumeric() || c == b'_' || c == b':' || c == b'$' => {
                i -= 1;
            }
            _ => {
                // Before giving up, check if there is an unclosed backtick
                // earlier on this line (user is still typing an escaped ident).
                let mut j = i - 1;
                let mut found_backtick = false;
                loop {
                    if bytes[j] == b'`' && !is_char_escaped(bytes, j) {
                        found_backtick = true;
                        i = j;
                        break;
                    }
                    if bytes[j].is_ascii_whitespace() {
                        break;
                    }
                    if j == 0 {
                        break;
                    }
                    j -= 1;
                }
                if !found_backtick {
                    break;
                }
            }
        }
    }

    (i, &text[i..cursor])
}

/// Checks whether the byte at `pos` is preceded by an odd number of
/// backslashes (i.e., the character is escaped).
pub(super) fn is_char_escaped(bytes: &[u8], pos: usize) -> bool {
    let mut count = 0u32;
    let mut j = pos;
    while j > 0 && bytes[j - 1] == b'\\' {
        count += 1;
        j -= 1;
    }
    count % 2 == 1
}

/// Advances `i` past a literal (double-quoted string, backtick identifier,
/// `//` line comment, `/regex/`, or `s/src/dst/` regex replace) if one starts
/// at `bytes[i]`. After returning `true`, `i` points at the closing delimiter
/// so the caller's `i += 1` skips past it.
fn skip_literal(bytes: &[u8], len: usize, i: &mut usize) -> bool {
    match bytes[*i] {
        b'"' | b'`' => {
            let delim = bytes[*i];
            *i += 1;
            while *i < len && bytes[*i] != delim {
                if bytes[*i] == b'\\' {
                    *i += 1;
                }
                *i += 1;
            }
            true
        }
        b'/' if *i + 1 < len && bytes[*i + 1] == b'/' => {
            while *i < len && bytes[*i] != b'\n' {
                *i += 1;
            }
            true
        }
        b'/' if preceded_by_eq(bytes, *i) => {
            skip_regex_body(bytes, len, i);
            true
        }
        b'/' if is_regex_replace_start(bytes, *i) => {
            skip_regex_body(bytes, len, i);
            skip_regex_body(bytes, len, i);
            true
        }
        _ => false,
    }
}

/// Advances `i` from an opening `/` past the regex body to the closing `/`,
/// handling `\` escapes. After return, `*i` points at the closing `/`.
fn skip_regex_body(bytes: &[u8], len: usize, i: &mut usize) {
    *i += 1;
    while *i < len && bytes[*i] != b'/' {
        if bytes[*i] == b'\\' {
            *i += 1;
        }
        *i += 1;
    }
}

/// Returns `true` when the non-whitespace character before `pos` is `=`
/// (covers both `==` and `!=` comparison operators preceding a regex).
fn preceded_by_eq(bytes: &[u8], pos: usize) -> bool {
    let mut j = pos;
    while j > 0 && bytes[j - 1].is_ascii_whitespace() {
        j -= 1;
    }
    j > 0 && bytes[j - 1] == b'='
}

/// Returns `true` when the `/` at `pos` is the opening of an `s/…/…/`
/// regex replace (always preceded by `~` in the grammar).
fn is_regex_replace_start(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 || bytes[pos - 1] != b's' {
        return false;
    }
    let mut j = pos - 1;
    while j > 0 && bytes[j - 1].is_ascii_whitespace() {
        j -= 1;
    }
    j > 0 && bytes[j - 1] == b'~'
}

fn find_last_pipe(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut last_pipe = None;
    let mut i = 0;
    while i < len {
        if !skip_literal(bytes, len, &mut i) && bytes[i] == b'|' {
            last_pipe = Some(i);
        }
        i += 1;
    }
    last_pipe
}

// ── source extraction ───────────────────────────────────────────

/// Extracts the dataset and metric name from the source portion of the query
/// using pest's `Rule::source` parser for correct backtick/escaping handling.
/// Expects text already scoped to the current subquery by `locate_query_context`.
fn extract_source_info(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    let len = bytes.len();

    // Find source portion: after directives, before first pipe
    let mut source_start = 0;
    let mut first_pipe = len;
    let mut i = 0;
    while i < len {
        if !skip_literal(bytes, len, &mut i) {
            match bytes[i] {
                b';' => source_start = i + 1,
                b'|' => {
                    first_pipe = i;
                    break;
                }
                _ => {}
            }
        }
        i += 1;
    }

    let source = text[source_start..first_pipe].trim();

    extract_source_via_parser(source)
}

/// Parses the source string using pest's `Rule::source` and extracts the
/// dataset and metric names from the resulting `metric_id` pair.
fn extract_source_via_parser(source: &str) -> Option<(String, String)> {
    let pairs = MPLParser::parse(Rule::source, source).ok()?;
    let source_pair = pairs.into_iter().next()?;

    let metric_id = source_pair
        .into_inner()
        .find(|p| p.as_rule() == Rule::metric_id)?;

    let mut dataset = None;
    let mut metric = None;
    for pair in metric_id.into_inner() {
        match pair.as_rule() {
            Rule::dataset => dataset = Some(extract_ident_name(pair)),
            Rule::metric_name => metric = Some(extract_ident_name(pair)),
            _ => {}
        }
    }

    let (dataset, metric) = (dataset?, metric?);
    if dataset.is_empty()
        || metric.is_empty()
        || dataset.starts_with('$')
        || metric.starts_with('$')
    {
        return None;
    }
    Some((dataset, metric))
}

/// Extracts the unescaped name from a `dataset` or `metric_name` pest pair.
/// Handles both `plain_ident` (raw text) and `escaped_ident` (backtick-wrapped,
/// descends into `escaped_ident_inner` to strip the backtick delimiters).
fn extract_ident_name(pair: pest::iterators::Pair<'_, Rule>) -> String {
    let Some(inner) = pair.into_inner().next() else {
        return String::new();
    };
    match inner.as_rule() {
        Rule::plain_ident | Rule::param_ident => inner.as_str().to_string(),
        Rule::escaped_ident => inner
            .into_inner()
            .next()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

// ── param extraction ────────────────────────────────────────────

/// Extracts declared parameters from the query preamble. Scans for
/// `param $name: type;` declarations that appear before the query body,
/// tolerating directives (`set ... ;`) and comments.
pub(super) fn extract_declared_params(text: &str) -> Vec<ParamItem> {
    let mut params = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("set ") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("param ") {
            if let Some(item) = parse_param_decl(rest) {
                params.push(item);
            }
            continue;
        }
        // First non-directive, non-param, non-comment line — stop scanning
        break;
    }
    params
}

/// Parses the remainder of a `param` declaration: `$name: type;`
fn parse_param_decl(rest: &str) -> Option<ParamItem> {
    let rest = rest.trim().strip_suffix(';')?.trim();
    let (name, typ_str) = rest.split_once(':')?;
    let name = name.trim();
    let typ_str = typ_str.trim();

    if !name.starts_with('$') {
        return None;
    }

    let typ = match typ_str {
        "Dataset" => ParamType::Dataset,
        "Metric" => ParamType::Metric,
        // `duration` is a legacy lowercase alias; `Duration` is canonical.
        "Duration" | "duration" => ParamType::Duration,
        "string" => ParamType::String,
        "int" => ParamType::Int,
        "float" => ParamType::Float,
        "bool" => ParamType::Bool,
        "Regex" => ParamType::Regex,
        _ => return None,
    };

    Some(ParamItem {
        label: name.to_string(),
        typ,
    })
}

/// Builds a `Params` completion result from the given params, filtered by
/// the allowed type predicate. Returns `None` if no params match.
fn suggest_params(
    span: Span,
    params: &[ParamItem],
    allowed: impl Fn(ParamType) -> bool,
) -> Option<CompletionResult> {
    let options: Vec<ParamItem> = params.iter().filter(|p| allowed(p.typ)).cloned().collect();

    if options.is_empty() {
        None
    } else {
        Some(CompletionResult::Params { span, options })
    }
}

// ── suggestion logic ────────────────────────────────────────────

/// Controls whether filter/where keywords are included in pipe completions.
/// The grammar's `compute_query` uses `pipe_rule*` (no filter) for the tail
/// after `compute_rule`, while simple queries use the full set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FilterPolicy {
    Include,
    Exclude,
}

// NOTE: `replace` and `join` are valid pipe keywords in the grammar's
// `pipe_rule` but are intentionally omitted here. The parser returns
// `ParseError::NotSupported` for both, so suggesting them would lead users
// to write queries that immediately fail. Add them here once parser and
// runtime support lands.
fn pipe_keywords(span: Span, policy: FilterPolicy, allow_sample: bool) -> CompletionResult {
    let mut options = Vec::with_capacity(9);
    if allow_sample {
        options.push(KeywordItem {
            label: "sample",
            apply: Some("sample "),
            info: "Sample time series at a numeric rate",
        });
    }
    if policy == FilterPolicy::Include {
        options.push(KeywordItem {
            label: "where",
            apply: Some("where "),
            info: "Filter time series by label values",
        });
    }
    options.extend([
        KeywordItem {
            label: "map",
            apply: Some("map "),
            info: "Apply a function to each data point",
        },
        KeywordItem {
            label: "group",
            apply: Some("group "),
            info: "Group time series by labels",
        },
        KeywordItem {
            label: "align",
            apply: Some("align "),
            info: "Align time series to a time grid",
        },
        KeywordItem {
            label: "bucket",
            apply: Some("bucket "),
            info: "Bucket time series into histogram buckets",
        },
        KeywordItem {
            label: "as",
            apply: Some("as "),
            info: "Rename the metric",
        },
    ]);
    CompletionResult::Keywords { span, options }
}

/// Completions for the `compute_rule` pipe position (first pipe after `}`).
/// Handles: `| compute <metric_name> using <compute_fn>`
fn suggest_for_compute_rule(text: &str, span: Span) -> Option<CompletionResult> {
    let pipe_pos = find_last_pipe(text)?;
    let after_pipe = text[pipe_pos + 1..].trim();

    if after_pipe.is_empty() {
        return Some(CompletionResult::Keywords {
            span,
            options: vec![KeywordItem {
                label: "compute",
                apply: Some("compute "),
                info: "Compute a new metric from two sources",
            }],
        });
    }

    let words: Vec<&str> = after_pipe.split_whitespace().collect();
    match words[0] {
        "compute" => match words.len() {
            1 => None,
            2 => Some(CompletionResult::Keywords {
                span,
                options: vec![KeywordItem {
                    label: "using",
                    apply: Some("using "),
                    info: "Specify the compute function",
                }],
            }),
            _ => {
                if *words.last()? == "using" {
                    Some(CompletionResult::ComputeFunctions {
                        span,
                        options: COMPUTE_COMPLETIONS.clone(),
                    })
                } else {
                    None
                }
            }
        },
        _ => None,
    }
}

fn suggest_for_context(
    before: &str,
    span: Span,
    policy: FilterPolicy,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    let pipe_pos = find_last_pipe(before)?;
    let after_pipe = before[pipe_pos + 1..].trim();

    // `sample` is only valid at the first pipe of a simple subquery
    let allow_sample = policy == FilterPolicy::Include && count_pipes(before) == 1;

    if after_pipe.is_empty() {
        return Some(pipe_keywords(span, policy, allow_sample));
    }

    let words: Vec<&str> = after_pipe.split_whitespace().collect();
    let first = words[0];
    let last = words.last().copied().unwrap_or(first);

    match first {
        "where" | "filter" if policy == FilterPolicy::Include => {
            suggest_filter_context(before, span, &words, last, params)
        }
        // `sample` takes a single numeric argument; no further completions
        "sample" => None,
        "group"
            if words.len() >= 2 && words[1] == "by" && (last == "by" || last.ends_with(',')) =>
        {
            let (dataset, metric) = extract_source_info(before)?;
            Some(CompletionResult::Tag {
                span,
                dataset,
                metric,
            })
        }
        _ => suggest_pipe_rule(first, last, &words, span, params),
    }
}

/// Shared logic for `pipe_rule` keyword completions (align/map/group/bucket/
/// as). Used by both simple queries and compute outer tails.
fn suggest_pipe_rule(
    first: &str,
    last: &str,
    words: &[&str],
    span: Span,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    match first {
        "align" => match last {
            "to" | "over" => suggest_params(span, params, |t| t == ParamType::Duration),
            "using" => Some(CompletionResult::AlignFunctions {
                span,
                options: ALIGN_COMPLETIONS.clone(),
            }),
            _ => {
                let has_to = words.contains(&"to");
                let has_over = words.contains(&"over");
                let mut options = Vec::new();
                if !has_to {
                    options.push(KeywordItem {
                        label: "to",
                        apply: Some("to "),
                        info: "Align to a time interval",
                    });
                }
                if has_to && !has_over {
                    options.push(KeywordItem {
                        label: "over",
                        apply: Some("over "),
                        info: "Specify the lookback window",
                    });
                }
                if has_to {
                    options.push(KeywordItem {
                        label: "using",
                        apply: Some("using "),
                        info: "Specify the align function",
                    });
                }
                Some(CompletionResult::Keywords { span, options })
            }
        },
        "map" => {
            if words.len() == 1 {
                return Some(CompletionResult::MapFunctions {
                    span,
                    options: MAP_COMPLETIONS.clone(),
                });
            }
            None
        }
        "group" => match last {
            "by" => None,
            "using" => Some(CompletionResult::GroupFunctions {
                span,
                options: GROUP_COMPLETIONS.clone(),
            }),
            _ if words.len() >= 2 && words[1] == "by" => Some(CompletionResult::Keywords {
                span,
                options: vec![KeywordItem {
                    label: "using",
                    apply: Some("using "),
                    info: "Specify the group function",
                }],
            }),
            _ => Some(CompletionResult::Keywords {
                span,
                options: vec![
                    KeywordItem {
                        label: "by",
                        apply: Some("by "),
                        info: "Group by labels",
                    },
                    KeywordItem {
                        label: "using",
                        apply: Some("using "),
                        info: "Specify the group function",
                    },
                ],
            }),
        },
        "bucket" => suggest_bucket_pipe(words, last, span, params),
        "as" => Some(CompletionResult::Keywords {
            span,
            options: vec![],
        }),
        _ => None,
    }
}

fn suggest_bucket_pipe(
    words: &[&str],
    last: &str,
    span: Span,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    if let Some(result) = suggest_bucket_args(words, span) {
        return Some(result);
    }
    match last {
        "by" => None,
        "to" => suggest_params(span, params, |t| t == ParamType::Duration),
        "using" => Some(CompletionResult::BucketFunctions {
            span,
            options: BUCKET_COMPLETIONS.clone(),
        }),
        _ => Some(CompletionResult::Keywords {
            span,
            options: vec![
                KeywordItem {
                    label: "by",
                    apply: Some("by "),
                    info: "Bucket by a label",
                },
                KeywordItem {
                    label: "to",
                    apply: Some("to "),
                    info: "Bucket to a target size",
                },
                KeywordItem {
                    label: "using",
                    apply: Some("using "),
                    info: "Specify the bucket function",
                },
            ],
        }),
    }
}

/// Recursively extracts enum keyword values from an `ArgType`, collecting
/// from `Enum`, `Repeated`, `OneOf`, and `Optional` variants.
fn extract_enum_values(arg_type: &ArgType) -> Vec<&'static str> {
    match arg_type {
        ArgType::Enum(values) => values.to_vec(),
        ArgType::Repeated { typ, .. } => extract_enum_values(typ),
        ArgType::OneOf(types) => types.iter().flat_map(extract_enum_values).collect(),
        ArgType::Optional(inner) => extract_enum_values(inner),
        ArgType::Float => vec![],
    }
}

/// Detects when the cursor is inside the parentheses of a bucket function
/// call and returns argument completions derived from the function's
/// `FunctionTrait::args()` metadata in the stdlib.
fn suggest_bucket_args(words: &[&str], span: Span) -> Option<CompletionResult> {
    let joined: String = words.join(" ");

    // Find an unmatched open paren (depth > 0 at end of string)
    let mut depth: i32 = 0;
    let mut last_open: Option<usize> = None;
    for (i, ch) in joined.char_indices() {
        match ch {
            '(' => {
                depth += 1;
                if depth == 1 {
                    last_open = Some(i);
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    last_open = None;
                }
            }
            _ => {}
        }
    }

    let open = last_open?;

    // Extract function name: identifier chars immediately before '('
    let before_paren = &joined[..open];
    let fn_name = before_paren
        .trim_end()
        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
        .next()
        .filter(|s| !s.is_empty())?;

    let func = STDLIB.bucket_functions.get(fn_name)?;
    let args = func.args();
    if args.is_empty() {
        return None;
    }

    let inside = &joined[open + 1..];
    let comma_count = inside.chars().filter(|&c| c == ',').count();

    // Determine which arg the cursor is on: if past the last positional arg,
    // clamp to the last arg if it is Repeated (variadic).
    let arg_idx = if comma_count < args.len() {
        comma_count
    } else {
        let last = args.len() - 1;
        if matches!(args[last].typ, ArgType::Repeated { .. }) {
            last
        } else {
            return None;
        }
    };

    let values = extract_enum_values(&args[arg_idx].typ);
    if values.is_empty() {
        return None;
    }

    Some(CompletionResult::Keywords {
        span,
        options: values
            .into_iter()
            .map(|v| KeywordItem {
                label: v,
                apply: None,
                info: "",
            })
            .collect(),
    })
}

fn suggest_filter_context(
    before: &str,
    span: Span,
    words: &[&str],
    last: &str,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    // Tag position: right after filter keyword, or after a boolean operator
    // NOTE: "not" and "(" overlap as logical grouping operators; both
    // trigger tag suggestions. A richer API would be needed to suggest
    // both boolean operators and tags simultaneously at the same position.
    if words.len() == 1 || matches!(last, "and" | "or" | "not" | "(") {
        let (dataset, metric) = extract_source_info(before)?;
        Some(CompletionResult::Tag {
            span,
            dataset,
            metric,
        })
    } else if words.len() > 2 {
        if last == "is" {
            return Some(CompletionResult::Keywords {
                span,
                options: vec![
                    KeywordItem {
                        label: "string",
                        apply: Some("string "),
                        info: "String type",
                    },
                    KeywordItem {
                        label: "int",
                        apply: Some("int "),
                        info: "Integer type",
                    },
                    KeywordItem {
                        label: "float",
                        apply: Some("float "),
                        info: "Float type",
                    },
                    KeywordItem {
                        label: "bool",
                        apply: Some("bool "),
                        info: "Boolean type",
                    },
                ],
            });
        }
        if matches!(last, "==" | "!=" | "<" | ">" | "<=" | ">=") {
            return suggest_filter_value_params(span, last, params);
        }
        Some(CompletionResult::Keywords {
            span,
            options: vec![
                KeywordItem {
                    label: "and",
                    apply: Some("and "),
                    info: "Logical AND",
                },
                KeywordItem {
                    label: "or",
                    apply: Some("or "),
                    info: "Logical OR",
                },
                KeywordItem {
                    label: "not",
                    apply: Some("not "),
                    info: "Logical NOT",
                },
            ],
        })
    } else {
        // words.len() == 2: tag name typed, suggest comparison operators
        Some(CompletionResult::Keywords {
            span,
            options: vec![
                KeywordItem {
                    label: "==",
                    apply: Some("== "),
                    info: "Equal",
                },
                KeywordItem {
                    label: "!=",
                    apply: Some("!= "),
                    info: "Not equal",
                },
                KeywordItem {
                    label: "<",
                    apply: Some("< "),
                    info: "Less than",
                },
                KeywordItem {
                    label: ">",
                    apply: Some("> "),
                    info: "Greater than",
                },
                KeywordItem {
                    label: "<=",
                    apply: Some("<= "),
                    info: "Less than or equal",
                },
                KeywordItem {
                    label: ">=",
                    apply: Some(">= "),
                    info: "Greater than or equal",
                },
                KeywordItem {
                    label: "is",
                    apply: Some("is "),
                    info: "Type check",
                },
            ],
        })
    }
}

/// Returns param completions for the filter value position (after a comparison
/// operator). Allows string/bool/int/float params for all operators, and regex
/// params only for `==` and `!=`.
fn suggest_filter_value_params(
    span: Span,
    op: &str,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    suggest_params(span, params, |typ| match typ {
        ParamType::String | ParamType::Bool | ParamType::Int | ParamType::Float => true,
        ParamType::Regex => matches!(op, "==" | "!="),
        _ => false,
    })
}

/// Returns `true` when every line in `text` is a preamble construct
/// (param declaration, set directive, comment, or blank). The cursor is
/// still in the preamble and no source/query text has been typed yet.
fn is_preamble_only(text: &str) -> bool {
    let mut has_preamble = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("set ") || trimmed.starts_with("param ")
        {
            has_preamble = true;
            continue;
        }
        return false;
    }
    has_preamble
}

/// Suggests completions when the cursor is in the preamble (before any query
/// source). Handles:
/// - Preamble keyword suggestions (`param`, `set`) when typing a prefix
/// - Suppression of source completions mid-declaration (`param `, `set `)
/// - Param type suggestions after `param $name: ` (the 8 valid types)
fn suggest_for_preamble(text: &str, partial: &str, span: Span) -> Option<CompletionResult> {
    if find_last_pipe(text).is_some() {
        return None;
    }

    // Find the current statement: after the last newline or semicolon.
    let stmt = text.rsplit(['\n', ';']).next().unwrap_or(text).trim_start();

    // Preamble keyword completion: cursor at statement start, partial is a
    // prefix of a preamble keyword, and all preceding lines are preamble.
    let lower = partial.to_ascii_lowercase();
    let matches_preamble_kw =
        !lower.is_empty() && ["param", "set"].iter().any(|kw| kw.starts_with(&lower));
    if stmt.is_empty() && matches_preamble_kw && is_preamble_position(text) {
        return Some(CompletionResult::Keywords {
            span,
            options: vec![
                KeywordItem {
                    label: "param",
                    apply: Some("param "),
                    info: "Declare a query parameter",
                },
                KeywordItem {
                    label: "set",
                    apply: Some("set "),
                    info: "Set a query option",
                },
            ],
        });
    }

    // Inside an incomplete `param` declaration
    if stmt == "param" || stmt.starts_with("param ") {
        let rest = stmt["param".len()..].trim_start();

        // After `param $name:` — suggest param types
        if let Some((name, _)) = rest.split_once(':')
            && name.trim().starts_with('$')
        {
            return Some(CompletionResult::Keywords {
                span,
                options: PARAM_TYPE_KEYWORDS.to_vec(),
            });
        }

        // Mid-declaration (e.g. `param `, `param $name`) — suppress source
        return Some(CompletionResult::Keywords {
            span,
            options: vec![],
        });
    }

    // Inside an incomplete `set` directive — suppress source
    if stmt == "set" || stmt.starts_with("set ") {
        return Some(CompletionResult::Keywords {
            span,
            options: vec![],
        });
    }

    None
}

/// Returns `true` when the text before the current partial consists entirely
/// of preamble constructs (param/set/comment/blank lines) or is empty — i.e.
/// the cursor is at a position where a new preamble keyword would be valid.
fn is_preamble_position(text: &str) -> bool {
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("set ") || trimmed.starts_with("param ")
        {
            continue;
        }
        return false;
    }
    true
}

const PARAM_TYPE_KEYWORDS: [KeywordItem; 8] = [
    KeywordItem {
        label: "Dataset",
        apply: Some("Dataset;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "Metric",
        apply: Some("Metric;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "Duration",
        apply: Some("Duration;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "string",
        apply: Some("string;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "int",
        apply: Some("int;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "float",
        apply: Some("float;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "bool",
        apply: Some("bool;\n"),
        info: "Parameter type",
    },
    KeywordItem {
        label: "Regex",
        apply: Some("Regex;\n"),
        info: "Parameter type",
    },
];

/// Suggests dataset or metric completions when the cursor is at the source
/// position (before any pipe). Returns `Dataset` when the user is typing the
/// dataset name, or `Metric` when they have typed `dataset:` and are typing
/// the metric name.
fn suggest_for_source(
    text: &str,
    partial: &str,
    span: Span,
    params: &[ParamItem],
) -> Option<CompletionResult> {
    // Only at source position — no pipe in the scoped text
    if find_last_pipe(text).is_some() {
        return None;
    }

    // When the partial is empty the cursor may still be in the preamble
    // (after param/set/comment lines). Don't suggest Dataset completions
    // there — the user hasn't started typing the source yet.
    if partial.is_empty() && is_preamble_only(text) {
        return None;
    }

    if let Some(colon_idx) = partial.find(':') {
        let dataset_raw = &partial[..colon_idx];
        if dataset_raw.is_empty() {
            return None;
        }
        // Metric part after the colon — param mode when it starts with `$`
        let metric_part = &partial[colon_idx + 1..];
        if metric_part.starts_with('$') {
            return suggest_params(Span::new(span.from + colon_idx + 1, span.to), params, |t| {
                t == ParamType::Metric
            });
        }
        let dataset = dataset_raw
            .strip_prefix('`')
            .and_then(|s| s.strip_suffix('`'))
            .unwrap_or(dataset_raw);
        // Skip past the opening backtick of the metric part if present
        let backtick_offset = usize::from(metric_part.starts_with('`'));
        Some(CompletionResult::Metric {
            span: Span::new(span.from + colon_idx + 1 + backtick_offset, span.to),
            dataset: dataset.to_string(),
        })
    } else if partial.starts_with('$') {
        suggest_params(span, params, |t| t == ParamType::Dataset)
    } else {
        // Skip past the opening backtick for unclosed escaped identifiers
        let backtick_offset = usize::from(partial.starts_with('`'));
        Some(CompletionResult::Dataset {
            span: Span::new(span.from + backtick_offset, span.to),
        })
    }
}

#[cfg(test)]
mod tests;
