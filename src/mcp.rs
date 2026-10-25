//! Optional local MCP server. All document access is marshalled to the UI thread.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
use libforma::geom::Affine;
use libforma::history::Command;
use libforma::model::{Geometry, Node, NodeLocation};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

const MCP_PORT: u16 = 43121;

pub enum Operation {
    ReadDocument,
    ListPages,
    ListLayers {
        page_id: Option<String>,
    },
    CreateRectangle {
        name: String,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    RenameLayer {
        id: String,
        name: String,
    },
    TransformLayer {
        id: String,
        x: f64,
        y: f64,
        scale_x: f64,
        scale_y: f64,
    },
    MoveLayer {
        id: String,
        page_id: String,
        parent_id: Option<String>,
        index: usize,
    },
    DeleteLayer {
        id: String,
    },
}

pub struct RequestMessage {
    pub operation: Operation,
    pub reply: oneshot::Sender<Result<Value, String>>,
}

#[derive(Clone)]
struct Server {
    requests: SyncSender<RequestMessage>,
    ui_context: eframe::egui::Context,
}

#[derive(Deserialize, JsonSchema)]
struct PageArgs {
    page_id: Option<String>,
}
#[derive(Deserialize, JsonSchema)]
struct CreateRectangleArgs {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
#[derive(Deserialize, JsonSchema)]
struct RenameArgs {
    id: String,
    name: String,
}
#[derive(Deserialize, JsonSchema)]
struct TransformArgs {
    id: String,
    x: f64,
    y: f64,
    scale_x: f64,
    scale_y: f64,
}
#[derive(Deserialize, JsonSchema)]
struct MoveArgs {
    id: String,
    page_id: String,
    parent_id: Option<String>,
    index: usize,
}
#[derive(Deserialize, JsonSchema)]
struct IdArgs {
    id: String,
}

#[tool_router]
impl Server {
    async fn call(&self, operation: Operation) -> Result<Value, ErrorData> {
        let (reply, receiver) = oneshot::channel();
        self.requests
            .try_send(RequestMessage { operation, reply })
            .map_err(|error| match error {
                mpsc::TrySendError::Full(_) => {
                    ErrorData::internal_error("Forma editor is busy; retry shortly", None)
                }
                mpsc::TrySendError::Disconnected(_) => {
                    ErrorData::internal_error("Forma editor is not available", None)
                }
            })?;
        self.ui_context.request_repaint();
        receiver
            .await
            .map_err(|_| ErrorData::internal_error("Forma editor closed the request", None))?
            .map_err(|error| ErrorData::internal_error(error, None))
    }

    #[tool(description = "Read the open design title, current page, page count, and revision.")]
    async fn read_document(
        &self,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::ReadDocument)
            .await
            .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "List all pages in the open design.")]
    async fn list_pages(&self) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::ListPages)
            .await
            .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "List the layer tree on a page. Omit page_id for the current page.")]
    async fn list_layers(
        &self,
        Parameters(args): Parameters<PageArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::ListLayers {
            page_id: args.page_id,
        })
        .await
        .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "Create a rectangle on the current page in page coordinates.")]
    async fn create_rectangle(
        &self,
        Parameters(args): Parameters<CreateRectangleArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::CreateRectangle {
            name: args.name,
            x: args.x,
            y: args.y,
            width: args.width,
            height: args.height,
        })
        .await
        .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "Rename a layer by its ID.")]
    async fn rename_layer(
        &self,
        Parameters(args): Parameters<RenameArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::RenameLayer {
            id: args.id,
            name: args.name,
        })
        .await
        .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "Set a layer's local position and scale.")]
    async fn transform_layer(
        &self,
        Parameters(args): Parameters<TransformArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::TransformLayer {
            id: args.id,
            x: args.x,
            y: args.y,
            scale_x: args.scale_x,
            scale_y: args.scale_y,
        })
        .await
        .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(
        description = "Move a layer to a page and optional parent container at a sibling index."
    )]
    async fn move_layer(
        &self,
        Parameters(args): Parameters<MoveArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::MoveLayer {
            id: args.id,
            page_id: args.page_id,
            parent_id: args.parent_id,
            index: args.index,
        })
        .await
        .map(rmcp::handler::server::wrapper::Json)
    }

    #[tool(description = "Delete a layer by its ID.")]
    async fn delete_layer(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<rmcp::handler::server::wrapper::Json<Value>, ErrorData> {
        self.call(Operation::DeleteLayer { id: args.id })
            .await
            .map(rmcp::handler::server::wrapper::Json)
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Forma local design editor. Editing tools change the open document and are undoable.",
        )
    }
}

#[derive(Clone)]
struct HttpState {
    service: rmcp::transport::StreamableHttpService<
        Server,
        rmcp::transport::streamable_http_server::session::local::LocalSessionManager,
    >,
    token: String,
}

async fn endpoint(State(state): State<HttpState>, request: Request<Body>) -> Response {
    if !request_is_authorized(request.headers(), &state.token) {
        return StatusCode::FORBIDDEN.into_response();
    }
    state.service.handle(request).await.into_response()
}

fn request_is_authorized(headers: &axum::http::HeaderMap, token: &str) -> bool {
    let bearer_ok = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        == Some(token);
    let host_ok = headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|h| {
            h == format!("127.0.0.1:{MCP_PORT}") || h == format!("localhost:{MCP_PORT}")
        });
    let origin_ok = headers.get(header::ORIGIN).is_none_or(|v| {
        v.to_str().ok().is_some_and(|o| {
            o == format!("http://127.0.0.1:{MCP_PORT}")
                || o == format!("http://localhost:{MCP_PORT}")
        })
    });
    bearer_ok && host_ok && origin_ok
}

pub struct RunningServer {
    pub receiver: Receiver<RequestMessage>,
    cancel: CancellationToken,
    thread: Option<JoinHandle<()>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn start(token: String, ui_context: eframe::egui::Context) -> Result<RunningServer, String> {
    let (sender, receiver) = mpsc::sync_channel(64);
    let cancel = CancellationToken::new();
    let thread_cancel = cancel.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let thread = thread::Builder::new()
        .name("forma-mcp".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    eprintln!("Forma MCP runtime failed: {err}");
                    return;
                }
            };
            runtime.block_on(async move {
                let config = rmcp::transport::StreamableHttpServerConfig::default()
                    .with_allowed_hosts([
                        format!("127.0.0.1:{MCP_PORT}"),
                        format!("localhost:{MCP_PORT}"),
                    ])
                    .with_cancellation_token(thread_cancel.clone());
                let service = rmcp::transport::StreamableHttpService::new(
                    move || {
                        Ok(Server {
                            requests: sender.clone(),
                            ui_context: ui_context.clone(),
                        })
                    },
                    Default::default(),
                    config,
                );
                let state = HttpState { service, token };
                let app = Router::new().route("/mcp", any(endpoint)).with_state(state);
                match tokio::net::TcpListener::bind(("127.0.0.1", MCP_PORT)).await {
                    Ok(listener) => {
                        let _ = ready_tx.send(Ok(()));
                        let _ = axum::serve(listener, app)
                            .with_graceful_shutdown(thread_cancel.cancelled_owned())
                            .await;
                    }
                    Err(err) => {
                        let _ = ready_tx.send(Err(format!("Could not bind MCP endpoint: {err}")));
                    }
                }
            });
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv_timeout(std::time::Duration::from_secs(3)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            cancel.cancel();
            let _ = thread.join();
            return Err(error);
        }
        Err(error) => {
            cancel.cancel();
            let _ = thread.join();
            return Err(format!("MCP server startup timed out: {error}"));
        }
    }
    Ok(RunningServer {
        receiver,
        cancel,
        thread: Some(thread),
    })
}

pub fn execute(
    editor: Option<&mut crate::editor::Editor>,
    operation: Operation,
) -> Result<Value, String> {
    let editor = editor.ok_or_else(|| "No design is open".to_owned())?;
    let result = match operation {
        Operation::ReadDocument => {
            json!({"title": editor.doc.title, "current_page_id": editor.page.to_string(), "page_count": editor.doc.pages.len(), "revision": editor.revision})
        }
        Operation::ListPages => json!(
            editor
                .doc
                .pages
                .iter()
                .map(|p| json!({"id": p.id.to_string(), "name": p.name}))
                .collect::<Vec<_>>()
        ),
        Operation::ListLayers { page_id } => {
            let page = match page_id {
                Some(id) => {
                    let id: libforma::PageId =
                        serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
                    editor.doc.page(id).ok_or("Page not found")?
                }
                None => editor.current_page(),
            };
            fn nodes(items: &[Node]) -> Vec<Value> {
                items.iter().map(|n| {
                    let kind = match n.kind {
                        libforma::model::NodeKind::Group { .. } => "group",
                        libforma::model::NodeKind::Frame(_) => "frame",
                        libforma::model::NodeKind::Shape(_) => "shape",
                        libforma::model::NodeKind::Text(_) => "text",
                        libforma::model::NodeKind::Image(_) => "image",
                    };
                    json!({"id":n.id.to_string(),"name":n.name,"kind":kind,"children":nodes(n.children())})
                }).collect()
            }
            json!(nodes(&page.children))
        }
        Operation::CreateRectangle {
            name,
            x,
            y,
            width,
            height,
        } => {
            if ![x, y, width, height].iter().all(|v| v.is_finite()) || width <= 0.0 || height <= 0.0
            {
                return Err("Coordinates must be finite and dimensions must be positive".into());
            }
            let mut node = Node::shape(Geometry::rect(width, height)).with_name(name);
            node.transform = Affine::translate((x, y));
            let id = node.id.to_string();
            let index = editor.current_page().children.len();
            if !editor.apply(Command::InsertNode {
                location: NodeLocation::top_level(editor.page, index),
                node,
            }) {
                return Err("Could not create rectangle".into());
            }
            json!({"id":id,"revision":editor.revision})
        }
        Operation::RenameLayer { id, name } => {
            let id: libforma::NodeId =
                serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
            if editor.doc.node(id).is_none() {
                return Err("Layer not found".into());
            }
            editor.rename_node(id, name);
            json!({"revision":editor.revision})
        }
        Operation::TransformLayer {
            id,
            x,
            y,
            scale_x,
            scale_y,
        } => {
            if ![x, y, scale_x, scale_y].iter().all(|v| v.is_finite())
                || scale_x == 0.0
                || scale_y == 0.0
            {
                return Err("Transform values must be finite and scales non-zero".into());
            }
            let id: libforma::NodeId =
                serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
            let old = editor.doc.node(id).ok_or("Layer not found")?.transform;
            let transform = Affine::translate((x, y))
                * Affine::scale_non_uniform(scale_x, scale_y)
                * Affine::translate((-old.translation().x, -old.translation().y));
            if !editor.apply(Command::SetTransform { id, transform }) {
                return Err("Could not transform layer".into());
            }
            json!({"revision":editor.revision})
        }
        Operation::MoveLayer {
            id,
            page_id,
            parent_id,
            index,
        } => {
            let id: libforma::NodeId =
                serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
            let page_id: libforma::PageId =
                serde_json::from_value(Value::String(page_id)).map_err(|e| e.to_string())?;
            let parent = parent_id
                .map(|s| serde_json::from_value(Value::String(s)).map_err(|e| e.to_string()))
                .transpose()?;
            if editor.doc.page(page_id).is_none() {
                return Err("Page not found".into());
            }
            if editor.doc.node(id).is_none() {
                return Err("Layer not found".into());
            }
            if parent.is_some_and(|p| editor.doc.node(p).is_none_or(|n| !n.is_container())) {
                return Err("Parent must be a group or frame".into());
            }
            if parent == Some(id) {
                return Err("A layer cannot be moved into itself".into());
            }
            if !editor.apply(Command::MoveNode {
                id,
                to: NodeLocation {
                    page: page_id,
                    parent,
                    index,
                },
            }) {
                return Err("Could not move layer".into());
            }
            json!({"revision":editor.revision})
        }
        Operation::DeleteLayer { id } => {
            let id: libforma::NodeId =
                serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
            if !editor.apply(Command::RemoveNode { id }) {
                return Err("Layer not found".into());
            }
            json!({"revision":editor.revision})
        }
    };
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Editor;

    #[test]
    fn http_requires_token_local_host_and_safe_origin() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer secret".parse().unwrap());
        headers.insert(
            header::HOST,
            format!("127.0.0.1:{MCP_PORT}").parse().unwrap(),
        );
        assert!(request_is_authorized(&headers, "secret"));
        assert!(!request_is_authorized(&headers, "wrong"));
        headers.insert(header::HOST, "attacker.example".parse().unwrap());
        assert!(!request_is_authorized(&headers, "secret"));
        headers.insert(
            header::HOST,
            format!("localhost:{MCP_PORT}").parse().unwrap(),
        );
        headers.insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
        assert!(!request_is_authorized(&headers, "secret"));
    }

    #[test]
    fn create_and_delete_are_undoable_and_advance_revision() {
        let mut editor = Editor::new_document("MCP test", None);
        let created = execute(
            Some(&mut editor),
            Operation::CreateRectangle {
                name: "Box".into(),
                x: 4.0,
                y: 8.0,
                width: 30.0,
                height: 20.0,
            },
        )
        .unwrap();
        let id = created["id"].as_str().unwrap().to_owned();
        assert_eq!(editor.revision, 1);
        assert_eq!(editor.current_page().children.len(), 1);
        execute(Some(&mut editor), Operation::DeleteLayer { id }).unwrap();
        assert_eq!(editor.current_page().children.len(), 0);
        assert_eq!(editor.revision, 2);
        editor.undo();
        assert_eq!(editor.current_page().children.len(), 1);
    }

    #[test]
    fn invalid_rectangle_does_not_change_document() {
        let mut editor = Editor::new_document("MCP test", None);
        let before = editor.revision;
        let result = execute(
            Some(&mut editor),
            Operation::CreateRectangle {
                name: "Invalid".into(),
                x: 0.0,
                y: 0.0,
                width: f64::NAN,
                height: 12.0,
            },
        );
        assert!(result.is_err());
        assert_eq!(editor.revision, before);
        assert!(editor.current_page().children.is_empty());
    }

    #[test]
    fn operations_require_an_open_document() {
        assert!(execute(None, Operation::ReadDocument).is_err());
    }
}
