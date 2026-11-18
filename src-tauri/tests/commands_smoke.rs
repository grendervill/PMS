//! Smoke tests for the Tauri command layer.
//!
//! The e2e suite (`tests/e2e`) mocks `invoke` in JavaScript, so nothing there
//! touches the real Rust commands: a renamed argument, a command that is never
//! registered in `generate_handler!` or a `Deserialize` that chokes on garbage
//! would only show up by hand, in the running app. These tests build the same
//! app on `tauri::test::MockRuntime` (`pms_lib::configure` is what `run()` uses,
//! `configure_mock` adds the command list) and push real IPC requests through the
//! command wrappers, plus compare the renderer's call sites against the Rust
//! signatures.
//!
//! What is covered:
//! * every command in `commands/` is registered in `generate_handler!`, and
//!   every command the renderer invokes exists;
//! * every argument name the renderer sends exists in the command signature
//!   (camelCase ↔ snake_case, `Option` parameters may be omitted);
//! * the registered commands answer on the mock runtime instead of
//!   "Command not found";
//! * argument values reach the bodies (`bad_params` for a bad length);
//! * expected error codes (`session_not_found`, `fs_scope`);
//! * garbage arguments are rejected without panicking, and the app stays usable.
//!
//! What is *not* covered, and why:
//! * commands that take an `app: AppHandle` cannot be registered on
//!   `MockRuntime`: `#[tauri::command]` expands the parameter to
//!   `AppHandle<Wry>`, so the wrapper only implements `CommandArg<'_, Wry>`.
//!   Their *registration* and *argument names* are still checked from the
//!   sources below; their bodies (native dialogs, biometrics, transfer, vault
//!   create/unlock/save) need a real window and are left to the e2e suite;
//! * plugin commands (`plugin:dialog|...` and friends) go through the ACL and a
//!   real window, not through `generate_handler!`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use pms_lib::configure_mock;
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::{App, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Root for every directory the app touches from its setup hook.
///
/// `MockRuntime` resolves `app_config_dir()` / `app_data_dir()` / `app_cache_dir()`
/// through the platform defaults, which would drop `fs-scope.json` and
/// `unlock-state.json` into the developer's real `Application Support`.
/// Redirecting the app directories to a temp folder keeps the machine clean.
fn app_root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("pms-commands-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create the test app directory");
        root
    })
}

/// The application `run()` starts, only on the mock runtime.
fn app() -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().identifier = "com.pms.smoke".to_string();
    context.config_mut().app.app_directories_override = Some(
        tauri::utils::config::AppDirectoriesOverride::Root(app_root().clone()),
    );

    configure_mock(mock_builder())
        .build(context)
        .expect("build the PMS app on the mock runtime")
}

fn window(app: &App<MockRuntime>) -> WebviewWindow<MockRuntime> {
    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .build()
        .expect("build the main webview window")
}

/// Sends one IPC request, exactly like the renderer's `invoke(cmd, args)`.
///
/// `Ok` is the deserialized response body (`Null` for raw payloads such as the
/// sealed vault file); `Err` is the rejection value: either an `ApiError` object
/// (`{ code, message }`) or the string Tauri's argument parser produces.
fn ipc(webview: &WebviewWindow<MockRuntime>, cmd: &str, args: Value) -> Result<Value, Value> {
    let request = InvokeRequest {
        cmd: cmd.to_string(),
        callback: CallbackFn(0),
        error: CallbackFn(1),
        url: "tauri://localhost".parse().expect("parse the webview url"),
        body: InvokeBody::Json(args),
        headers: Default::default(),
        invoke_key: INVOKE_KEY.to_string(),
    };

    get_ipc_response(webview, request)
        .map(|body| body.deserialize::<Value>().unwrap_or(Value::Null))
}

fn message_of(error: &Value) -> String {
    match error {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn code_of(error: &Value) -> Option<&str> {
    error.get("code").and_then(Value::as_str)
}

// ---------------------------------------------------------------------------
// Source scanning helpers
// ---------------------------------------------------------------------------

fn package_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_dir_sources(dir: &Path, extensions: &[&str], skip_tests: bool) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries = std::fs::read_dir(&current)
            .unwrap_or_else(|error| panic!("read {}: {error}", current.display()));
        for entry in entries {
            let path = entry.expect("read a directory entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            if skip_tests && name.contains(".test.") {
                continue;
            }
            if !extensions.iter().any(|ext| name.ends_with(ext)) {
                continue;
            }
            let source = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
            files.push((path.display().to_string(), source));
        }
    }

    files.sort();
    files
}

fn skip_whitespace(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn is_command_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() >= 3
        && matches!(chars.next(), Some(first) if first.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// One `invoke("...")` / `call<T>("...")` call site in the renderer.
struct RendererCall {
    command: String,
    /// Top-level keys of the argument object; empty for calls without one.
    args: Vec<String>,
}

/// Extracts `invoke(...)` / `call(...)` call sites from TypeScript source.
///
/// Hand-rolled instead of a regex: the generic parameter list may itself contain
/// quotes (`invoke<{ meta: import("@/…").AttachmentMeta }>(`), so `<...>` is
/// skipped before the first string literal is read. Calls whose command name is
/// a variable (`invoke(command, args)` in the `call` wrapper) are not commands
/// and are ignored.
fn renderer_calls(source: &str) -> Vec<RendererCall> {
    let bytes = source.as_bytes();
    let mut calls = Vec::new();

    for callee in ["invoke", "call"] {
        let mut index = 0;
        while let Some(found) = source[index..].find(callee) {
            let start = index + found;
            index = start + callee.len();

            let mut cursor = index;
            if bytes.get(cursor) == Some(&b'<') {
                let mut depth = 0usize;
                while cursor < bytes.len() {
                    match bytes[cursor] {
                        b'<' => depth += 1,
                        b'>' => {
                            depth -= 1;
                            if depth == 0 {
                                cursor += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    cursor += 1;
                }
            }

            cursor = skip_whitespace(bytes, cursor);
            if bytes.get(cursor) != Some(&b'(') {
                continue;
            }
            cursor = skip_whitespace(bytes, cursor + 1);
            if bytes.get(cursor) != Some(&b'"') {
                continue;
            }
            let rest = &source[cursor + 1..];
            let Some(end) = rest.find('"') else { continue };
            let command = &rest[..end];
            if !is_command_name(command) {
                continue;
            }
            index = cursor + 1 + end;

            cursor = skip_whitespace(bytes, index + 1);
            if bytes.get(cursor) == Some(&b',') {
                cursor = skip_whitespace(bytes, cursor + 1);
            }

            let args = match bytes.get(cursor) {
                Some(b'{') => {
                    let mut depth = 0usize;
                    let mut closing = cursor;
                    while closing < bytes.len() {
                        match bytes[closing] {
                            b'{' => depth += 1,
                            b'}' => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        closing += 1;
                    }
                    object_keys(&source[cursor + 1..closing])
                }
                // `invoke("cmd")` — no arguments.
                _ => Vec::new(),
            };

            calls.push(RendererCall {
                command: command.to_string(),
                args,
            });
        }
    }

    calls
}

/// Top-level keys of a TypeScript object literal body.
fn object_keys(body: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    let mut parts = Vec::new();

    for ch in body.chars() {
        match ch {
            '{' | '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '}' | ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => parts.push(std::mem::take(&mut current)),
            _ => current.push(ch),
        }
    }
    parts.push(current);

    for part in parts {
        let part = part.trim();
        if part.is_empty() || part.starts_with("...") {
            continue;
        }
        // `key: value` — a ternary in the value also has a colon, but the key
        // always comes first, so the first colon at depth 0 wins.
        let mut depth = 0usize;
        let mut key = part;
        for (offset, ch) in part.char_indices() {
            match ch {
                '{' | '(' | '[' => depth += 1,
                '}' | ')' | ']' => depth -= 1,
                ':' if depth == 0 => {
                    key = &part[..offset];
                    break;
                }
                _ => {}
            }
        }
        let key = key.trim().trim_matches(['"', '\'']);
        if !key.is_empty() {
            keys.push(key.to_string());
        }
    }

    keys
}

/// A `#[tauri::command]` function.
struct Command {
    /// Parameter names as the renderer spells them (camelCase), without the
    /// injected `AppHandle` / `State` parameters.
    args: Vec<String>,
    /// Parameters the renderer must send (anything that is not `Option<...>`).
    required_args: Vec<String>,
    /// The command takes an `AppHandle`, so its wrapper only works on wry.
    needs_app_handle: bool,
}

fn to_camel_case(name: &str) -> String {
    let name = name.trim_start_matches('_');
    let mut parts = name.split('_');
    let mut out = parts.next().unwrap_or_default().to_string();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Parses every `#[tauri::command]` in the given Rust sources.
fn command_signatures(sources: &[(String, String)]) -> BTreeMap<String, Command> {
    let mut commands = BTreeMap::new();

    for (file, source) in sources {
        let mut rest = source.as_str();
        while let Some(offset) = rest.find("#[tauri::command") {
            rest = &rest[offset..];
            let attribute_end = rest
                .find(']')
                .unwrap_or_else(|| panic!("unterminated command attribute in {file}"));
            let after_attribute = &rest[attribute_end + 1..];
            let Some(fn_offset) = after_attribute.find("fn ") else {
                panic!("no `fn` after the command attribute in {file}");
            };
            let signature = &after_attribute[fn_offset + 3..];
            let name_end = signature
                .find(|c: char| c == '(' || c.is_whitespace() || c == '<')
                .unwrap_or(signature.len());
            let name = signature[..name_end].trim().to_string();

            let open = signature
                .find('(')
                .unwrap_or_else(|| panic!("`{name}` in {file} has no parameter list"));
            let params = &signature[open + 1..];
            let mut depth = 1usize;
            let mut end = params.len();
            for (index, ch) in params.char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = index;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let param_list = &params[..end];

            let mut args = Vec::new();
            let mut required_args = Vec::new();
            let mut needs_app_handle = false;
            for param in split_top_level(param_list) {
                let param = param.trim();
                if param.is_empty() {
                    continue;
                }
                let (raw_name, raw_type) = match param.split_once(':') {
                    Some((name, ty)) => (name.trim(), ty.trim()),
                    None => continue,
                };
                if ["AppHandle", "State", "Window", "Webview"]
                    .iter()
                    .any(|injected| raw_type.contains(injected))
                {
                    needs_app_handle |= raw_type.contains("AppHandle");
                    continue;
                }
                let arg = to_camel_case(raw_name);
                if !raw_type.starts_with("Option") {
                    required_args.push(arg.clone());
                }
                args.push(arg);
            }

            commands.insert(
                name,
                Command {
                    args,
                    required_args,
                    needs_app_handle,
                },
            );
            rest = &params[end..];
        }
    }

    commands
}

fn split_top_level(list: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in list.char_indices() {
        match ch {
            '(' | '<' | '[' => depth += 1,
            ')' | '>' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&list[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&list[start..]);
    parts
}

/// Command names listed in a `generate_handler![]` block.
fn handler_commands(source: &str) -> Vec<String> {
    source
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("commands::") && line.ends_with(','))
        .filter_map(|line| line.rsplit("::").next())
        .map(|name| name.trim_end_matches(',').to_string())
        .collect()
}

/// The two handler lists in `lib.rs`: the mock-safe one and the full one.
fn handler_lists(lib: &str) -> (Vec<String>, Vec<String>) {
    let mock_start = lib
        .find("pub fn configure_mock")
        .expect("lib.rs no longer has configure_mock");
    let run_start = lib
        .find("pub fn run()")
        .expect("lib.rs no longer has run()");
    assert!(
        mock_start < run_start,
        "configure_mock must come before run"
    );

    (
        handler_commands(&lib[mock_start..run_start]),
        handler_commands(&lib[run_start..]),
    )
}

fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names.dedup();
    names
}

// ---------------------------------------------------------------------------
// 1. Registration
// ---------------------------------------------------------------------------

#[test]
fn registered_commands_answer_instead_of_not_found() {
    let lib = include_str!("../src/lib.rs");
    let (mock, _) = handler_lists(lib);
    assert!(
        mock.len() > 20,
        "the mock handler list was not parsed: {mock:?}"
    );

    let app = app();
    let webview = window(&app);

    for command in mock {
        // `transfer_scan` really scans the network, so it gets the shortest
        // allowed duration instead of an empty object (which would use the
        // 4 second default).
        let args = if command == "transfer_scan" {
            json!({ "durationMs": 1 })
        } else {
            json!({})
        };

        match ipc(&webview, &command, args) {
            // Commands without required arguments (`generator_wordlist`,
            // `transfer_receive_stop`) really run and answer; `ApiResult<()>`
            // comes back as JSON `null`.
            Ok(_) => {}
            Err(error) => {
                let message = message_of(&error);
                assert!(
                    !message.contains("not found"),
                    "{command} is not in generate_handler!: {message}"
                );
                // Both shapes below are produced *after* the name matched a
                // handler: an `ApiError` from the body, or Tauri's
                // "invalid args `x` for command `y`" from the argument parser.
                assert!(
                    code_of(&error).is_some() || message.starts_with("invalid args"),
                    "{command}: unexpected rejection: {message}"
                );
            }
        }
    }
}

#[test]
fn unknown_command_is_reported_as_not_found() {
    let app = app();
    let webview = window(&app);

    let error = ipc(&webview, "no_such_command", json!({})).expect_err("nobody handles this");
    assert!(
        message_of(&error).contains("not found"),
        "expected a not-found rejection, got {}",
        message_of(&error)
    );
}

#[test]
fn every_command_is_registered_and_the_mock_list_is_the_handle_free_subset() {
    let commands = command_signatures(&read_dir_sources(
        &package_root().join("src/commands"),
        &[".rs"],
        false,
    ));
    let (mock, full) = handler_lists(include_str!("../src/lib.rs"));

    // A command defined in `commands/` but missing from `generate_handler!` is
    // exactly the bug that only shows up as "Command not found" in the UI.
    assert_eq!(
        sorted(commands.keys().cloned().collect()),
        sorted(full.clone()),
        "every #[tauri::command] must be listed in generate_handler! in run()"
    );

    // ... and the list used by the mock runtime must be the full one minus the
    // commands whose wrapper is tied to `AppHandle<Wry>`.
    let handle_free: Vec<String> = full
        .iter()
        .filter(|name| {
            commands
                .get(*name)
                .is_some_and(|command| !command.needs_app_handle)
        })
        .cloned()
        .collect();
    assert!(
        handle_free.len() > 20,
        "no handle-free commands parsed: {handle_free:?}"
    );
    assert_eq!(
        sorted(mock.clone()),
        sorted(handle_free),
        "configure_mock must list every command that does not take an AppHandle"
    );
}

// ---------------------------------------------------------------------------
// 1b. Raw-bytes commands
// ---------------------------------------------------------------------------

/// Both raw commands must be in `generate_handler!`; only the one without an
/// `AppHandle` can also live in the mock list (the runtime split is described in
/// `lib.rs`).
#[test]
fn raw_vault_commands_are_registered() {
    let (mock, full) = handler_lists(include_str!("../src/lib.rs"));

    assert!(
        full.contains(&"vault_unlock_raw".to_string()),
        "vault_unlock_raw must be registered in run(): {full:?}"
    );
    assert!(
        full.contains(&"vault_inspect_raw".to_string()),
        "vault_inspect_raw must be registered in run(): {full:?}"
    );
    assert!(mock.contains(&"vault_inspect_raw".to_string()));
    assert!(
        !mock.contains(&"vault_unlock_raw".to_string()),
        "vault_unlock_raw takes an AppHandle, so the mock list must not have it"
    );
}

/// The mock runtime can only send JSON bodies (raw bodies exist on wry and not
/// on Android either), so the command has to answer with `raw_unavailable`
/// instead of panicking on the unexpected `InvokeBody::Json`.
#[test]
fn raw_vault_commands_refuse_a_json_body() {
    let app = app();
    let webview = window(&app);

    let error =
        ipc(&webview, "vault_inspect_raw", json!({ "fileB64": "AAAA" })).expect_err("raw only");
    assert_eq!(
        code_of(&error),
        Some("raw_unavailable"),
        "{}",
        message_of(&error)
    );

    // The base64 command is still there next to it, and still validates input.
    let base64 =
        ipc(&webview, "vault_inspect", json!({ "fileB64": "AAAA" })).expect_err("not a vault file");
    assert_eq!(
        code_of(&base64),
        Some("truncated"),
        "{}",
        message_of(&base64)
    );
}

// ---------------------------------------------------------------------------
// 2. Argument names
// ---------------------------------------------------------------------------

#[test]
fn generate_password_accepts_camel_case_options() {
    let app = app();
    let webview = window(&app);

    let generated = ipc(
        &webview,
        "generate_password",
        json!({
            "options": {
                "length": 16,
                "lowercase": true,
                "uppercase": true,
                "digits": true,
                "symbols": true,
            }
        }),
    )
    .expect("generate_password must answer with a password");

    let password = generated.as_str().expect("the password is a string");
    assert_eq!(
        password.chars().count(),
        16,
        "unexpected password {password:?}"
    );

    // An out-of-range length proves the body actually saw the value: if the
    // argument name or one of the fields stopped matching, this would be an
    // "invalid args" reject instead of `bad_params`.
    let error = ipc(
        &webview,
        "generate_password",
        json!({ "options": { "length": 3 } }),
    )
    .expect_err("length 3 is out of range");
    assert_eq!(
        code_of(&error),
        Some("bad_params"),
        "{}",
        message_of(&error)
    );
}

#[test]
fn renderer_argument_names_match_the_command_signatures() {
    let renderer = read_dir_sources(&package_root().join("../src"), &[".ts", ".tsx"], true);
    let commands = command_signatures(&read_dir_sources(
        &package_root().join("src/commands"),
        &[".rs"],
        false,
    ));

    let calls: Vec<RendererCall> = renderer
        .iter()
        .flat_map(|(_, source)| renderer_calls(source))
        .collect();
    assert!(
        calls.len() > 40,
        "only {} call sites were parsed",
        calls.len()
    );

    let problems = argument_problems(&calls, &commands);
    assert!(
        problems.is_empty(),
        "renderer/command argument mismatch:\n{}",
        problems.join("\n")
    );
}

/// Compares every renderer call site with the command signature it targets.
fn argument_problems(calls: &[RendererCall], commands: &BTreeMap<String, Command>) -> Vec<String> {
    let mut problems = Vec::new();

    for call in calls {
        let Some(command) = commands.get(&call.command) else {
            problems.push(format!(
                "the renderer calls `{}`, which is not a #[tauri::command]",
                call.command
            ));
            continue;
        };
        if call.args.is_empty() {
            // No argument object at all: every parameter must be optional.
            for required in &command.required_args {
                problems.push(format!(
                    "`{}` is called without arguments but requires `{required}`",
                    call.command
                ));
            }
            continue;
        }
        for arg in &call.args {
            if !command.args.contains(arg) {
                problems.push(format!(
                    "`{arg}` is not a parameter of `{}` (signature has {:?})",
                    call.command, command.args
                ));
            }
        }
    }

    problems
}

// ---------------------------------------------------------------------------
// 3. Session and filesystem scope
// ---------------------------------------------------------------------------

#[test]
fn unknown_session_answers_session_not_found() {
    let app = app();
    let webview = window(&app);

    // `sessionId` is the camelCase name the renderer sends; a mismatch would
    // surface as "invalid args" instead.
    let error = ipc(
        &webview,
        "vault_lock",
        json!({ "sessionId": "00000000-0000-4000-8000-0000000000ff" }),
    )
    .expect_err("there is no such session");

    assert_eq!(
        code_of(&error),
        Some("session_not_found"),
        "{}",
        message_of(&error)
    );
}

#[test]
fn paths_outside_the_grants_are_rejected() {
    let app = app();
    let webview = window(&app);

    let outside = std::env::temp_dir().join("pms-not-a-grant");
    std::fs::create_dir_all(&outside).expect("create the folder");
    assert!(
        !outside.starts_with(app_root()),
        "the probe folder must stay outside the app directories"
    );

    let error = ipc(
        &webview,
        "vault_remember_folder",
        json!({ "folder": outside.to_string_lossy() }),
    )
    .expect_err("the folder was never picked in a native dialog");

    assert_eq!(code_of(&error), Some("fs_scope"), "{}", message_of(&error));
}

// ---------------------------------------------------------------------------
// 4. Deserialization never panics
// ---------------------------------------------------------------------------

#[test]
fn garbage_arguments_are_rejected_without_killing_the_app() {
    let app = app();
    let webview = window(&app);

    let garbage = vec![
        ("vault_inspect", json!({ "fileB64": "not base64 !!" })),
        ("vault_inspect", json!({ "fileB64": 42 })),
        ("keyfile_kind", json!({ "fileB64": [] })),
        ("vault_lock", json!({ "sessionId": "not-a-uuid" })),
        ("vault_lock", json!({})),
        ("storage_read_file", json!({ "path": 1 })),
        ("storage_write_atomic", json!({ "path": null, "b64": [] })),
        ("storage_mkdir", json!({ "path": {}, "mode": "rw" })),
        ("storage_list_dir", json!({ "path": "../../etc" })),
        ("generate_password", json!({ "options": null })),
        ("generate_pin", json!({ "options": { "length": -5 } })),
        (
            "generate_passphrase",
            json!({ "options": { "words": "many" } }),
        ),
        ("generator_entropy", json!({ "input": null })),
        ("totp_code", json!({ "config": {}, "at": "now" })),
        ("totp_inspect", json!({ "input": 0 })),
        ("share_decode", json!({ "payload": 1, "password": {} })),
        ("ssh_check", json!({ "directory": [], "name": null })),
        ("ssh_install", json!({ "directory": null })),
        ("import_parse", json!({ "fileB64": {}, "password": [] })),
        ("pms_import", json!({ "archive": 1, "destination": 2 })),
        ("favicon_fetch", json!({ "host": [] })),
        ("password_breach_count", json!({ "password": null })),
        ("vault_remember_folder", json!({ "folder": "" })),
        ("transfer_scan", json!({ "durationMs": "soon" })),
    ];

    for (command, args) in garbage {
        let result = ipc(&webview, command, args);
        if let Err(error) = result {
            let message = message_of(&error);
            // A panic inside a command surfaces as a join error, which would
            // otherwise pass silently.
            assert!(
                !message.contains("panicked"),
                "{command} panicked: {message}"
            );
        }
    }

    // The process, the managed state and the sessions are still usable: this is
    // the real assertion — a panic or a poisoned lock would break it.
    let wordlist = ipc(&webview, "generator_wordlist", json!({}))
        .expect("the app must still answer after garbage arguments");
    assert!(
        wordlist
            .get("builtinWords")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0,
        "unexpected wordlist info {wordlist}"
    );

    let password = ipc(
        &webview,
        "generate_password",
        json!({ "options": { "length": 12 } }),
    )
    .expect("generation still works");
    assert_eq!(password.as_str().map(str::len), Some(12));
}

// ---------------------------------------------------------------------------
// 5. The scanners themselves
// ---------------------------------------------------------------------------

#[test]
fn scanners_parse_what_they_are_supposed_to() {
    // Guards the parsers above: a broken scanner would make the source checks
    // pass by finding nothing.
    let typescript = r#"
        await invoke<string>("generate_password", { options });
        await call<void>("vault_remember_folder", { folder });
        await invoke<Array<{ name: string; isFile: boolean }>>("storage_list_dir", { path });
        await invoke<{ meta: import("@/features/vault/types").AttachmentMeta }>(
            "attachment_seal_store",
            { sessionId, folder, b64, name, mime },
        );
        await invoke(command, args);
    "#;

    let calls = renderer_calls(typescript);
    let names: Vec<String> = sorted(calls.iter().map(|call| call.command.clone()).collect());
    assert_eq!(
        names,
        vec![
            "attachment_seal_store",
            "generate_password",
            "storage_list_dir",
            "vault_remember_folder",
        ]
    );
    let seal_store = calls
        .iter()
        .find(|call| call.command == "attachment_seal_store")
        .expect("the multi-line call was parsed");
    assert_eq!(
        seal_store.args,
        vec!["sessionId", "folder", "b64", "name", "mime"]
    );

    let rust = r#"
        #[tauri::command]
        pub fn plain(name: String, at: Option<u64>) -> ApiResult<()> { todo!() }

        #[tauri::command]
        pub async fn with_app(
            app: AppHandle,
            file_b64: String,
            state: State<'_, AppState>,
        ) -> ApiResult<()> { todo!() }
    "#;
    let commands = command_signatures(&[("test.rs".to_string(), rust.to_string())]);
    let plain = commands.get("plain").expect("plain parsed");
    assert_eq!(plain.args, vec!["name", "at"]);
    assert_eq!(plain.required_args, vec!["name"]);
    assert!(!plain.needs_app_handle);
    let with_app = commands.get("with_app").expect("with_app parsed");
    assert_eq!(with_app.args, vec!["fileB64"]);
    assert!(with_app.needs_app_handle);

    let lib = include_str!("../src/lib.rs");
    let (mock, full) = handler_lists(lib);
    assert!(
        mock.len() < full.len(),
        "the two handler lists were conflated"
    );

    // A renamed argument or a command that does not exist must be reported —
    // this is the bug class the whole file exists for.
    let drifted = renderer_calls(
        r#"
        await call<void>("generate_password", { opts });
        await call<void>("vault_typo", { sessionId });
        await call<void>("vault_lock");
        "#,
    );
    let mut signatures = command_signatures(&[(
        "test.rs".to_string(),
        r#"
        #[tauri::command]
        pub fn generate_password(options: PasswordOptions) -> ApiResult<String> { todo!() }
        #[tauri::command]
        pub fn vault_lock(session_id: Uuid) -> ApiResult<()> { todo!() }
        "#
        .to_string(),
    )]);
    signatures.extend(commands);
    let problems = argument_problems(&drifted, &signatures);
    assert_eq!(problems.len(), 3, "expected three problems: {problems:#?}");
    assert!(problems[0].contains("`opts` is not a parameter of `generate_password`"));
    assert!(problems[1].contains("`vault_typo`"));
    assert!(problems[2].contains("requires `sessionId`"));
}
