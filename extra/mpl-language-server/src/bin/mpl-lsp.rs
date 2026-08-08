use eyre::Result;
use lsp_server::{
    Connection, ErrorCode, IoThreads, Message, Notification as ServerNotification,
    Request as ServerRequest, Response,
};
use lsp_types::notification::{
    DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, Notification,
    PublishDiagnostics,
};
use lsp_types::request::{Completion as CompletionRequest, Request as LspRequest};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    CompletionTextEdit, Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, Documentation, InitializeResult,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, ServerInfo,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions, TextEdit,
};
use mpl_language_server::{
    CompletionResult, Severity as MplSeverity, Span, compute_completions_with_params,
    compute_diagnostics_raw,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

fn main() -> Result<()> {
    let server = Server::new();

    server.run()?;

    Ok(())
}

struct Server {
    connection: Connection,
    io_threads: IoThreads,
    shutdown: AtomicBool,
    documents: HashMap<String, String>,
}

impl Server {
    fn new() -> Self {
        let (connection, io_threads) = Connection::stdio();
        let shutdown = AtomicBool::new(false);
        let documents = HashMap::new();

        Self {
            connection,
            io_threads,
            shutdown,
            documents,
        }
    }

    fn initialize_result() -> InitializeResult {
        InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(TextDocumentSyncKind::FULL),
                        ..Default::default()
                    },
                )),
                completion_provider: Some(CompletionOptions::default()),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "mpl-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        }
    }

    fn run(mut self) -> Result<()> {
        let (id, _params) = self.connection.initialize_start()?;
        let init = serde_json::to_value(Self::initialize_result())?;
        self.connection.initialize_finish(id, init)?;

        while let Ok(message) = self.connection.receiver.recv() {
            self.handle_message(message)?;

            if self.shutdown.load(Ordering::Relaxed) {
                break;
            }
        }

        self.io_threads.join()?;

        Ok(())
    }

    fn handle_message(&mut self, message: Message) -> Result<()> {
        match message {
            Message::Request(request) => self.handle_request(request),
            Message::Notification(notification) => self.handle_notification(notification),
            Message::Response(response) => self.handle_response(response),
        }
    }

    fn handle_request(&self, request: ServerRequest) -> Result<()> {
        if self.connection.handle_shutdown(&request)? {
            self.shutdown.store(true, Ordering::Release);
            return Ok(());
        }

        match request.method.as_str() {
            CompletionRequest::METHOD => {
                let (id, params) = request.extract(CompletionRequest::METHOD)?;
                let result = self.completions(params);
                let response = Response::new_ok(id, result);
                self.connection.sender.send(response.into())?;
            }
            _ => {
                let id = request.id;
                let error = ErrorCode::MethodNotFound;
                let response = Response::new_err(id, error as i32, "method not found".to_string());
                self.connection.sender.send(response.into())?;
            }
        }

        Ok(())
    }

    fn handle_notification(&mut self, notification: ServerNotification) -> Result<()> {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params = notification.extract(DidOpenTextDocument::METHOD)?;
                self.did_open_txt_doc(params)?;
            }
            DidChangeTextDocument::METHOD => {
                let params = notification.extract(DidChangeTextDocument::METHOD)?;
                self.did_change_txt_doc(params)?;
            }
            DidCloseTextDocument::METHOD => {
                let params = notification.extract(DidCloseTextDocument::METHOD)?;
                self.did_close_txt_doc(params)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_response(&self, _response: Response) -> Result<()> {
        Ok(())
    }
}

impl Server {
    fn did_open_txt_doc(&mut self, params: DidOpenTextDocumentParams) -> Result<()> {
        let text = params.text_document.text;
        let uri = params.text_document.uri;

        let diagnostics = self.diagnostics(&text);

        self.documents.insert(uri.as_str().to_owned(), text);

        let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
        let notification = ServerNotification::new(PublishDiagnostics::METHOD.to_string(), params);

        self.connection.sender.send(notification.into())?;

        Ok(())
    }

    fn did_change_txt_doc(&mut self, params: DidChangeTextDocumentParams) -> Result<()> {
        let Some(change) = params.content_changes.into_iter().last() else {
            return Ok(());
        };

        let text = change.text;
        let uri = params.text_document.uri;

        let diagnostics = self.diagnostics(&text);

        self.documents.insert(uri.as_str().to_owned(), text);

        let params = PublishDiagnosticsParams::new(uri, diagnostics, None);
        let notification = ServerNotification::new(PublishDiagnostics::METHOD.to_string(), params);

        self.connection.sender.send(notification.into())?;

        Ok(())
    }

    fn did_close_txt_doc(&mut self, params: DidCloseTextDocumentParams) -> Result<()> {
        let uri = params.text_document.uri;

        self.documents.remove(uri.as_str());

        let params = PublishDiagnosticsParams::new(uri, Vec::new(), None);
        let notification = ServerNotification::new(PublishDiagnostics::METHOD.to_string(), params);

        self.connection.sender.send(notification.into())?;

        Ok(())
    }
}

impl Server {
    fn completions(&self, params: CompletionParams) -> Option<CompletionResponse> {
        let text = self
            .documents
            .get(params.text_document_position.text_document.uri.as_str())?;

        let cursor = position_to_byte(text, params.text_document_position.position);

        let completions = compute_completions_with_params(text, cursor, &[])
            .into_iter()
            .flat_map(|completion| match completion {
                CompletionResult::Keywords { span, options } => options
                    .into_iter()
                    .map(|item| {
                        let insert = item.apply.unwrap_or(item.label).to_string();
                        let doc = Documentation::String(item.info.to_string());
                        let edit = TextEdit::new(span_to_range(text, span), insert);
                        let completion = CompletionTextEdit::Edit(edit);

                        CompletionItem {
                            label: item.label.to_string(),
                            kind: Some(CompletionItemKind::KEYWORD),
                            documentation: Some(doc),
                            text_edit: Some(completion),
                            ..Default::default()
                        }
                    })
                    .collect(),
                CompletionResult::AlignFunctions { span, options }
                | CompletionResult::MapFunctions { span, options }
                | CompletionResult::GroupFunctions { span, options }
                | CompletionResult::BucketFunctions { span, options }
                | CompletionResult::ComputeFunctions { span, options } => options
                    .into_iter()
                    .map(|item| {
                        let doc = Documentation::String(item.label.to_string());
                        let edit = TextEdit::new(span_to_range(text, span), item.label.to_string());
                        let completion = CompletionTextEdit::Edit(edit);

                        CompletionItem {
                            label: item.label.to_string(),
                            kind: Some(CompletionItemKind::FUNCTION),
                            documentation: Some(doc),
                            text_edit: Some(completion),
                            ..Default::default()
                        }
                    })
                    .collect(),
                CompletionResult::Params { span, options } => options
                    .into_iter()
                    .map(|item| {
                        let edit = TextEdit::new(span_to_range(text, span), item.label.to_string());
                        let completion = CompletionTextEdit::Edit(edit);

                        CompletionItem {
                            label: item.label.to_string(),
                            kind: Some(CompletionItemKind::VARIABLE),
                            text_edit: Some(completion),
                            ..Default::default()
                        }
                    })
                    .collect(),
                CompletionResult::Tag { .. }
                | CompletionResult::Dataset { .. }
                | CompletionResult::Metric { .. } => Vec::new(),
            })
            .collect();

        Some(CompletionResponse::Array(completions))
    }

    fn diagnostics(&self, text: &str) -> Vec<Diagnostic> {
        compute_diagnostics_raw(text, HashMap::new())
            .iter()
            .map(|diagnostic| {
                let severity = severity_map(&diagnostic.severity);
                let range = span_to_range(text, diagnostic.span);
                let message = match &diagnostic.help {
                    Some(help) if !help.is_empty() => format!("{}\n{help}", diagnostic.message),
                    _ => diagnostic.message.to_string(),
                };

                Diagnostic {
                    range,
                    severity: Some(severity),
                    source: Some("mpl".to_string()),
                    message,
                    ..Default::default()
                }
            })
            .collect()
    }
}

fn severity_map(severity: &MplSeverity) -> DiagnosticSeverity {
    match severity {
        MplSeverity::Error => DiagnosticSeverity::ERROR,
        MplSeverity::Warning => DiagnosticSeverity::WARNING,
        MplSeverity::Info => DiagnosticSeverity::INFORMATION,
        MplSeverity::Hint => DiagnosticSeverity::HINT,
    }
}

fn span_to_range(text: &str, span: Span) -> Range {
    Range::new(
        byte_to_position(text, span.from),
        byte_to_position(text, span.to),
    )
}

fn byte_to_position(text: &str, offset: usize) -> Position {
    let offset = offset.min(text.len());
    let mut line = 0;
    let mut character = 0;

    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += ch.len_utf16() as u32;
        }
    }

    Position::new(line, character)
}

fn position_to_byte(text: &str, position: Position) -> usize {
    let mut line = 0;
    let mut character = 0;

    for (idx, ch) in text.char_indices() {
        if line == position.line {
            if character >= position.character {
                return idx;
            }

            if ch == '\n' {
                return idx;
            }

            let next = character + ch.len_utf16() as u32;
            if next > position.character {
                return idx;
            }
            character = next;
            continue;
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        }
    }

    text.len()
}
