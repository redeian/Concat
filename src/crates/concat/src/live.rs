// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! Live mode: the API editing the project the window has on screen.
//!
//! The API's own sessions are separate documents, which is why a caller has
//! never been able to touch what the window shows. This module is the other
//! half of the bridge that lifts that: the API hands a request naming the
//! open project here, this hops to the window's event loop with
//! [`slint::invoke_from_event_loop`], runs the same [`Command`] the
//! inspector would run against the window's own [`Studio::session`], lets
//! the studio do its usual bookkeeping and republishes the models, then
//! hands the reply back to the waiting caller.
//!
//! Three properties this keeps on purpose:
//!
//! - **One document.** Nothing is copied: the command lands on the session
//!   the window is drawing, so the timeline, the monitor and the undo
//!   history are the user's own, and ⌘Z in the window undoes an agent's
//!   edit like any other.
//! - **The event loop stays the owner.** Every call is queued as a task on
//!   the same loop the pointer events go through, so a request cannot be
//!   applied halfway through a drag: it waits its turn.
//! - **A closed window is an error, not a hang.** The caller waits at most
//!   [`PATIENCE`]; a loop that has gone away says so at once.

use std::sync::mpsc;
use std::time::Duration;

use concat_api::{ApiError, ErrorCode, Reply, Request, Response, WindowBridge};
use concat_project::Command;

use crate::host::Shell;

/// How long a caller waits for the event loop to get to its request. A loop
/// busy exporting is still answering other calls between frames, so this is
/// a broken-window guard, not a budget.
const PATIENCE: Duration = Duration::from_secs(60);

/// The window as the API's live target: a unit struct, because everything
/// it needs is the process's one [`Shell`].
pub struct Window;

impl WindowBridge for Window {
    fn call(&self, request: Request) -> Result<Response, ApiError> {
        let (sent, reply) = mpsc::channel();
        // Queued, not run here: `Shell::with` would find nothing on this
        // thread, and the session it holds is not `Sync`.
        slint::invoke_from_event_loop(move || {
            let mut answer = None;
            Shell::with(|shell, app| {
                answer = handle(&shell.studio, request);
                // Published whatever happened: an edit landed on the
                // document the window draws, and the toast a refusal raised
                // belongs on screen too.
                let studio = shell.studio.borrow();
                studio.publish(&app, &shell.models);
            });
            let _ = sent.send(answer);
        })
        .map_err(|_| ApiError::failed("the window is closing".to_owned()))?;
        match reply.recv_timeout(PATIENCE) {
            Ok(Some(answer)) => Ok(Response::from(answer)),
            // The shell was never installed or the window is gone: the
            // loop ran but there was nothing to run against.
            Ok(None) => Err(ApiError::new(
                ErrorCode::Refused,
                "the window has no project open".to_owned(),
            )),
            Err(_) => Err(ApiError::failed(
                "the window did not answer in time".to_owned(),
            )),
        }
    }
}

/// Runs one request against the window's session. `None` when there is no
/// shell to run it in.
fn handle(
    studio: &std::cell::RefCell<crate::studio::Studio>,
    request: Request,
) -> Option<Result<Reply, ApiError>> {
    let mut studio = studio.borrow_mut();
    let open = studio.session.as_ref().map(|session| session.path().to_owned());
    let open = match open {
        Some(open) => open,
        None => {
            return Some(Err(ApiError::new(
                ErrorCode::Refused,
                "the window has no project open".to_owned(),
            )));
        }
    };
    // One session is on screen at a time, so a request naming any other
    // folder is not the window's to answer - even where the register still
    // says the window holds it.
    let names_open = named_path(&request).is_some_and(|path| same_folder(path, &open));
    if !names_open {
        return Some(Err(ApiError::new(
            ErrorCode::Refused,
            format!("{open} is the project on screen, and that request names another"),
        )));
    }
    let answer = match request {
        // Opening what is already open is reading it, as it is in the API.
        Request::ProjectOpen { .. } | Request::ProjectGet { .. } => {
            Ok(view(studio.session.as_ref().expect("checked").view()))
        }
        Request::ProjectDocument { .. } => Ok(Reply::Document(
            studio.session.as_ref().expect("checked").document(),
        )),
        Request::EditApply { command, .. } => {
            let applied = studio.session.as_mut().expect("checked").apply(*command);
            match applied {
                Ok(applied) => {
                    studio.note_change();
                    Ok(view(applied))
                }
                Err(error) => Err(ApiError::new(ErrorCode::Refused, error)),
            }
        }
        Request::EditUndo { .. } => {
            let undone = studio.session.as_mut().expect("checked").undo();
            studio.note_change();
            Ok(view(undone))
        }
        Request::EditRedo { .. } => {
            let redone = studio.session.as_mut().expect("checked").redo();
            studio.note_change();
            Ok(view(redone))
        }
        Request::ProjectSave { name, .. } => {
            let saved = studio
                .session
                .as_mut()
                .expect("checked")
                .save(name.as_deref());
            match saved {
                Ok(()) => Ok(Reply::Done(concat_api::Done {})),
                Err(error) => Err(ApiError::failed(error)),
            }
        }
        Request::ProjectSetVideo { video, .. } => {
            let applied = studio
                .session
                .as_mut()
                .expect("checked")
                .set_video(video);
            match applied {
                Ok(applied) => {
                    studio.note_change();
                    Ok(view(applied))
                }
                Err(error) => Err(ApiError::new(ErrorCode::Refused, error)),
            }
        }
        Request::MediaImport { file, .. } => {
            let item = match concat_host::media::probe(&file) {
                Ok(probed) => probed.to_new_media(),
                Err(error) => return Some(Err(ApiError::failed(error))),
            };
            let applied = studio
                .session
                .as_mut()
                .expect("checked")
                .apply(Command::AddMedia { item });
            match applied {
                Ok(applied) => {
                    studio.note_change();
                    Ok(view(applied))
                }
                Err(error) => Err(ApiError::new(ErrorCode::Refused, error)),
            }
        }
        // The window closes its own project: a caller asking it to would
        // be closing the tab the user is working in, with no way back.
        Request::ProjectClose { .. } => Err(ApiError::new(
            ErrorCode::Refused,
            format!("{open} is the project on screen: close it in the window"),
        )),
        // Rendered and exported from the window's own slot, which serves
        // the pointer: these need the live render path, not the document.
        Request::PreviewFrame { .. } | Request::ExportRun { .. } => Err(ApiError::new(
            ErrorCode::Refused,
            "live mode is on for edits and reads; a frame or an export of the project on screen is not wired yet - save it and export from the window, or close it there and let the agent take it".to_owned(),
        )),
        Request::MediaProbe { .. }
        | Request::TemplateSave { .. }
        | Request::ExportCancel { .. }
        | Request::ExportStatus { .. } => Err(ApiError::new(
            ErrorCode::Refused,
            "that request does not name the project on screen".to_owned(),
        )),
        Request::Version
        | Request::ProjectCreate { .. }
        | Request::ProjectList
        | Request::CatalogueList { .. }
        | Request::TemplateList
        | Request::TemplateInstantiate { .. } => {
            unreachable!("the API answers these itself, they name no project")
        }
    };
    Some(answer)
}

fn view(view: concat_host::session::EditorView) -> Reply {
    Reply::View(Box::new(view))
}

/// The project folder a request names, when it names one.
fn named_path(request: &Request) -> Option<&str> {
    match request {
        Request::ProjectOpen { path }
        | Request::ProjectClose { path, .. }
        | Request::ProjectGet { path }
        | Request::ProjectDocument { path }
        | Request::ProjectSave { path, .. }
        | Request::ProjectSetVideo { path, .. }
        | Request::EditApply { path, .. }
        | Request::EditUndo { path }
        | Request::EditRedo { path }
        | Request::MediaImport { path, .. }
        | Request::TemplateSave { path, .. }
        | Request::ExportRun { path, .. }
        | Request::PreviewFrame { path, .. } => Some(path),
        _ => None,
    }
}

/// Whether two paths name the same folder, without canonicalising a path
/// that may not exist any more: the trailing separator is the only
/// difference a caller and the window ever have.
fn same_folder(left: &str, right: &str) -> bool {
    left.trim_end_matches('/') == right.trim_end_matches('/')
}
