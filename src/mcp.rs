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
use libforma::model::{Geometry, Node, NodeKind, NodeLocation, TextAlign, TextStyle};
use libforma::paint::{Color, Paint, Stroke};
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ErrorData, ServerHandler, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
    CreateShape(CreateShapeArgs),
    CreateText(CreateTextArgs),
    CreateFrame(CreateFrameArgs),
    SetFill {
        id: String,
        fill: PaintArgs,
    },
    ClearFill {
        id: String,
    },
    SetStroke {
        id: String,
        color: String,
        width: f64,
    },
    SetOpacity {
        id: String,
        opacity: f32,
    },
    SetCornerRadius {
        id: String,
        radius: f64,
    },
    UpdateText {
        id: String,
        content: String,
    },
    DuplicateLayer {
        id: String,
        offset_x: f64,
        offset_y: f64,
    },
    SetVisibility {
        id: String,
        visible: bool,
    },
    Undo,
    Redo,
    GroupLayers {
        ids: Vec<String>,
        name: String,
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

#[derive(Clone, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum PaintArgs {
    Solid {
        color: String,
    },
    LinearGradient {
        start: [f64; 2],
        end: [f64; 2],
        stops: Vec<GradientStopArgs>,
    },
    RadialGradient {
        center: [f64; 2],
        radius: f64,
        stops: Vec<GradientStopArgs>,
    },
}
#[derive(Clone, Deserialize, JsonSchema)]
pub(crate) struct GradientStopArgs {
    offset: f32,
    color: String,
}
#[derive(Clone, Deserialize, JsonSchema)]
pub(crate) struct CreateShapeArgs {
    name: String,
    shape: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    #[serde(default)]
    corner_radius: f64,
    fill: Option<PaintArgs>,
    stroke_color: Option<String>,
    #[serde(default)]
    stroke_width: f64,
}
#[derive(Clone, Deserialize, JsonSchema)]
pub(crate) struct CreateTextArgs {
    name: String,
    content: String,
    x: f64,
    y: f64,
    #[serde(default = "default_font_family")]
    font_family: String,
    #[serde(default = "default_font_size")]
    font_size: f64,
    #[serde(default = "default_font_weight")]
    font_weight: u16,
    #[serde(default = "default_text_color")]
    color: String,
    #[serde(default)]
    align: String,
    width: Option<f64>,
}
fn default_font_family() -> String {
    "Inter".into()
}
fn default_font_size() -> f64 {
    16.0
}
fn default_font_weight() -> u16 {
    400
}
fn default_text_color() -> String {
    "#000000".into()
}
#[derive(Clone, Deserialize, JsonSchema)]
pub(crate) struct CreateFrameArgs {
    name: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    fill: Option<PaintArgs>,
}
#[derive(Deserialize, JsonSchema)]
struct SetFillArgs {
    id: String,
    fill: PaintArgs,
}
#[derive(Deserialize, JsonSchema)]
struct SetStrokeArgs {
    id: String,
    color: String,
    width: f64,
}
#[derive(Deserialize, JsonSchema)]
struct SetOpacityArgs {
    id: String,
    opacity: f32,
}
#[derive(Deserialize, JsonSchema)]
struct SetRadiusArgs {
    id: String,
    radius: f64,
}
#[derive(Deserialize, JsonSchema)]
struct UpdateTextArgs {
    id: String,
    content: String,
}
#[derive(Deserialize, JsonSchema)]
struct DuplicateArgs {
    id: String,
    #[serde(default)]
    offset_x: f64,
    #[serde(default)]
    offset_y: f64,
}
#[derive(Deserialize, JsonSchema)]
struct VisibilityArgs {
    id: String,
    visible: bool,
}
#[derive(Deserialize, JsonSchema)]
struct GroupArgs {
    ids: Vec<String>,
    name: String,
}

#[derive(Serialize, JsonSchema)]
struct ToolOutput {
    data: Value,
}

#[tool_router]
impl Server {
    async fn call(&self, operation: Operation) -> Result<ToolOutput, ErrorData> {
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
            .map(|data| ToolOutput { data })
            .map_err(|error| ErrorData::internal_error(error, None))
    }

    #[tool(description = "Read the open design title, current page, page count, and revision.")]
    async fn read_document(&self) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::ReadDocument).await.map(Json)
    }

    #[tool(description = "List all pages in the open design.")]
    async fn list_pages(&self) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::ListPages).await.map(Json)
    }

    #[tool(description = "List the layer tree on a page. Omit page_id for the current page.")]
    async fn list_layers(
        &self,
        Parameters(args): Parameters<PageArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::ListLayers {
            page_id: args.page_id,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Create a rectangle on the current page in page coordinates.")]
    async fn create_rectangle(
        &self,
        Parameters(args): Parameters<CreateRectangleArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::CreateRectangle {
            name: args.name,
            x: args.x,
            y: args.y,
            width: args.width,
            height: args.height,
        })
        .await
        .map(Json)
    }

    #[tool(
        description = "Create a rectangle, ellipse, or line with optional fill and stroke on the current page. Coordinates are page coordinates; dimensions are positive."
    )]
    async fn create_shape(
        &self,
        Parameters(args): Parameters<CreateShapeArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::CreateShape(args)).await.map(Json)
    }

    #[tool(
        description = "Create a text layer on the current page with font and paragraph settings."
    )]
    async fn create_text(
        &self,
        Parameters(args): Parameters<CreateTextArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::CreateText(args)).await.map(Json)
    }

    #[tool(description = "Create a frame on the current page with optional fill.")]
    async fn create_frame(
        &self,
        Parameters(args): Parameters<CreateFrameArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::CreateFrame(args)).await.map(Json)
    }

    #[tool(description = "Set a shape fill to a solid color, linear gradient, or radial gradient.")]
    async fn set_fill(
        &self,
        Parameters(args): Parameters<SetFillArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::SetFill {
            id: args.id,
            fill: args.fill,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Remove the fill from a shape.")]
    async fn clear_fill(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::ClearFill { id: args.id })
            .await
            .map(Json)
    }

    #[tool(description = "Set or replace a shape's solid stroke color and width.")]
    async fn set_stroke(
        &self,
        Parameters(args): Parameters<SetStrokeArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::SetStroke {
            id: args.id,
            color: args.color,
            width: args.width,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Set a layer's opacity from 0 to 1.")]
    async fn set_opacity(
        &self,
        Parameters(args): Parameters<SetOpacityArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::SetOpacity {
            id: args.id,
            opacity: args.opacity,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Set a rectangle shape's corner radius.")]
    async fn set_corner_radius(
        &self,
        Parameters(args): Parameters<SetRadiusArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::SetCornerRadius {
            id: args.id,
            radius: args.radius,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Replace the content of a text layer.")]
    async fn update_text(
        &self,
        Parameters(args): Parameters<UpdateTextArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::UpdateText {
            id: args.id,
            content: args.content,
        })
        .await
        .map(Json)
    }

    #[tool(
        description = "Duplicate a layer and its descendants, offsetting it in its parent's coordinates."
    )]
    async fn duplicate_layer(
        &self,
        Parameters(args): Parameters<DuplicateArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::DuplicateLayer {
            id: args.id,
            offset_x: args.offset_x,
            offset_y: args.offset_y,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Set a layer's visibility.")]
    async fn set_visibility(
        &self,
        Parameters(args): Parameters<VisibilityArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::SetVisibility {
            id: args.id,
            visible: args.visible,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Undo the most recent design change.")]
    async fn undo(&self) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::Undo).await.map(Json)
    }

    #[tool(description = "Redo the most recently undone design change.")]
    async fn redo(&self) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::Redo).await.map(Json)
    }

    #[tool(
        description = "Group two or more consecutive sibling layers on the same page or in the same parent container."
    )]
    async fn group_layers(
        &self,
        Parameters(args): Parameters<GroupArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::GroupLayers {
            ids: args.ids,
            name: args.name,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Rename a layer by its ID.")]
    async fn rename_layer(
        &self,
        Parameters(args): Parameters<RenameArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::RenameLayer {
            id: args.id,
            name: args.name,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Set a layer's local position and scale.")]
    async fn transform_layer(
        &self,
        Parameters(args): Parameters<TransformArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::TransformLayer {
            id: args.id,
            x: args.x,
            y: args.y,
            scale_x: args.scale_x,
            scale_y: args.scale_y,
        })
        .await
        .map(Json)
    }

    #[tool(
        description = "Move a layer to a page and optional parent container at a sibling index."
    )]
    async fn move_layer(
        &self,
        Parameters(args): Parameters<MoveArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::MoveLayer {
            id: args.id,
            page_id: args.page_id,
            parent_id: args.parent_id,
            index: args.index,
        })
        .await
        .map(Json)
    }

    #[tool(description = "Delete a layer by its ID.")]
    async fn delete_layer(
        &self,
        Parameters(args): Parameters<IdArgs>,
    ) -> Result<Json<ToolOutput>, ErrorData> {
        self.call(Operation::DeleteLayer { id: args.id })
            .await
            .map(Json)
    }
}

#[tool_handler]
impl ServerHandler for Server {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Forma local design editor. Tools read the active document and can create text, shapes and frames; edit fills, gradients, strokes, opacity, and layer properties; organize layers; and undo or redo. Editing tools change the open document and are undoable. Tool payloads are returned in the structured data field.",
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

fn color_stroke(color: &str, width: f64) -> Result<Stroke, String> {
    if !width.is_finite() || width < 0.0 {
        return Err("Stroke width must be finite and non-negative".into());
    }
    let color = Color::from_hex(color).ok_or_else(|| format!("Invalid color: {color}"))?;
    Ok(Stroke::new(Paint::solid(color), width))
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
    fn parsed_paint(args: PaintArgs) -> Result<Paint, String> {
        let color = |s: &str| Color::from_hex(s).ok_or_else(|| format!("Invalid color: {s}"));
        let stops =
            |items: Vec<GradientStopArgs>| -> Result<Vec<libforma::paint::GradientStop>, String> {
                if items.is_empty() {
                    return Err("A gradient needs at least one color stop".into());
                }
                let mut out = Vec::with_capacity(items.len());
                let mut previous = -1.0f32;
                for stop in items {
                    if !stop.offset.is_finite()
                        || !(0.0..=1.0).contains(&stop.offset)
                        || stop.offset < previous
                    {
                        return Err(
                            "Gradient stops must be ordered and have offsets from 0 to 1".into(),
                        );
                    }
                    previous = stop.offset;
                    out.push(libforma::paint::GradientStop::new(
                        stop.offset,
                        color(&stop.color)?,
                    ));
                }
                Ok(out)
            };
        match args {
            PaintArgs::Solid { color: value } => Ok(Paint::solid(color(&value)?)),
            PaintArgs::LinearGradient {
                start,
                end,
                stops: values,
            } => {
                if !start.iter().chain(end.iter()).all(|v| v.is_finite()) {
                    return Err("Gradient coordinates must be finite".into());
                }
                Ok(Paint::LinearGradient {
                    start: libforma::geom::Point::new(start[0], start[1]),
                    end: libforma::geom::Point::new(end[0], end[1]),
                    stops: stops(values)?,
                })
            }
            PaintArgs::RadialGradient {
                center,
                radius,
                stops: values,
            } => {
                if !center.iter().all(|v| v.is_finite()) || !radius.is_finite() || radius <= 0.0 {
                    return Err("Gradient center must be finite and radius positive".into());
                }
                Ok(Paint::RadialGradient {
                    center: libforma::geom::Point::new(center[0], center[1]),
                    radius,
                    stops: stops(values)?,
                })
            }
        }
    }
    fn update(
        editor: &mut crate::editor::Editor,
        id: &str,
        edit: impl FnOnce(&mut Node),
    ) -> Result<u64, String> {
        let id: libforma::NodeId =
            serde_json::from_value(Value::String(id.to_owned())).map_err(|e| e.to_string())?;
        let mut node = editor.doc.node(id).cloned().ok_or("Layer not found")?;
        edit(&mut node);
        if let Some(children) = node.children_mut() {
            children.clear();
        }
        if !editor.apply(Command::UpdateNode { node }) {
            return Err("Could not update layer".into());
        }
        Ok(editor.revision)
    }
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
                    let mut detail=json!({"id":n.id.to_string(),"name":n.name,"kind":kind,"visible":n.visible,"locked":n.locked,"opacity":n.opacity,"transform":[n.transform.as_coeffs()[0],n.transform.as_coeffs()[1],n.transform.as_coeffs()[2],n.transform.as_coeffs()[3],n.transform.as_coeffs()[4],n.transform.as_coeffs()[5]],"children":nodes(n.children())});
                    match &n.kind {
                        NodeKind::Text(text) => detail["text"]=json!({"content":text.content,"font_family":text.style.font_family,"font_size":text.style.font_size,"font_weight":text.style.font_weight,"color":text.style.color,"align":text.align,"width":text.width}),
                        NodeKind::Shape(shape) => { detail["geometry"]=serde_json::to_value(&shape.geometry).unwrap_or(Value::Null); detail["fill"]=serde_json::to_value(&shape.fill).unwrap_or(Value::Null); detail["stroke"]=serde_json::to_value(&shape.stroke).unwrap_or(Value::Null); },
                        NodeKind::Frame(frame) => { detail["size"]=json!([frame.size.width,frame.size.height]); detail["fill"]=serde_json::to_value(&frame.fill).unwrap_or(Value::Null); detail["clip"]=json!(frame.clip); },
                        _ => {}
                    }
                    detail
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
        Operation::CreateShape(args) => {
            if ![
                args.x,
                args.y,
                args.width,
                args.height,
                args.corner_radius,
                args.stroke_width,
            ]
            .iter()
            .all(|v| v.is_finite())
                || args.width <= 0.0
                || args.height <= 0.0
                || args.corner_radius < 0.0
                || args.stroke_width < 0.0
            {
                return Err("Shape coordinates must be finite, dimensions positive, and radius/stroke width non-negative".into());
            }
            let geometry = match args.shape.as_str() {
                "rectangle" => Geometry::rounded_rect(args.width, args.height, args.corner_radius),
                "ellipse" if args.corner_radius == 0.0 => {
                    Geometry::ellipse(args.width, args.height)
                }
                "line" if args.corner_radius == 0.0 => Geometry::Line {
                    end: libforma::geom::Point::new(args.width, args.height),
                },
                "ellipse" | "line" => return Err("Corner radius applies only to rectangles".into()),
                _ => return Err("Shape must be rectangle, ellipse, or line".into()),
            };
            let fill = args.fill.map(parsed_paint).transpose()?;
            let stroke = args
                .stroke_color
                .as_deref()
                .map(|c| color_stroke(c, args.stroke_width))
                .transpose()?;
            let mut node = Node::shape(geometry).with_name(args.name);
            node.transform = Affine::translate((args.x, args.y));
            if let NodeKind::Shape(shape) = &mut node.kind {
                shape.fill = fill;
                shape.stroke = stroke;
            }
            let id = node.id.to_string();
            let index = editor.current_page().children.len();
            if !editor.apply(Command::InsertNode {
                location: NodeLocation::top_level(editor.page, index),
                node,
            }) {
                return Err("Could not create shape".into());
            }
            json!({"id":id,"revision":editor.revision})
        }
        Operation::CreateText(args) => {
            if !args.x.is_finite()
                || !args.y.is_finite()
                || !args.font_size.is_finite()
                || args.font_size <= 0.0
                || !(100..=900).contains(&args.font_weight)
                || args.width.is_some_and(|w| !w.is_finite() || w <= 0.0)
            {
                return Err("Text position, size, weight, or width is invalid".into());
            }
            let color = Color::from_hex(&args.color)
                .ok_or_else(|| format!("Invalid color: {}", args.color))?;
            let align = match args.align.as_str() {
                "" | "start" => TextAlign::Start,
                "center" => TextAlign::Center,
                "end" => TextAlign::End,
                "justify" => TextAlign::Justify,
                _ => return Err("Text alignment must be start, center, end, or justify".into()),
            };
            let style = TextStyle {
                font_family: args.font_family,
                font_size: args.font_size,
                font_weight: args.font_weight,
                color,
                ..TextStyle::default()
            };
            let mut node = Node::text(args.content, style).with_name(args.name);
            node.transform = Affine::translate((args.x, args.y));
            if let NodeKind::Text(text) = &mut node.kind {
                text.width = args.width;
                text.align = align;
            }
            let id = node.id.to_string();
            let index = editor.current_page().children.len();
            if !editor.apply(Command::InsertNode {
                location: NodeLocation::top_level(editor.page, index),
                node,
            }) {
                return Err("Could not create text".into());
            }
            json!({"id":id,"revision":editor.revision})
        }
        Operation::CreateFrame(args) => {
            if ![args.x, args.y, args.width, args.height]
                .iter()
                .all(|v| v.is_finite())
                || args.width <= 0.0
                || args.height <= 0.0
            {
                return Err("Frame coordinates must be finite and dimensions positive".into());
            }
            let fill = args.fill.map(parsed_paint).transpose()?;
            let mut node = Node::frame(args.name, args.width, args.height);
            node.transform = Affine::translate((args.x, args.y));
            if let NodeKind::Frame(frame) = &mut node.kind {
                frame.fill = fill;
            }
            let id = node.id.to_string();
            let index = editor.current_page().children.len();
            if !editor.apply(Command::InsertNode {
                location: NodeLocation::top_level(editor.page, index),
                node,
            }) {
                return Err("Could not create frame".into());
            }
            json!({"id":id,"revision":editor.revision})
        }
        Operation::SetFill { id, fill } => {
            let paint = parsed_paint(fill)?;
            let node_id: libforma::NodeId =
                serde_json::from_value(Value::String(id.clone())).map_err(|e| e.to_string())?;
            if !editor
                .doc
                .node(node_id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Shape(_) | NodeKind::Frame(_)))
            {
                return Err("Fill can only be set on shape or frame layers".into());
            }
            let revision = update(editor, &id, |node| match &mut node.kind {
                NodeKind::Shape(shape) => shape.fill = Some(paint),
                NodeKind::Frame(frame) => frame.fill = Some(paint),
                _ => unreachable!("target type validated"),
            })?;
            json!({"revision":revision})
        }
        Operation::ClearFill { id } => {
            let node_id: libforma::NodeId =
                serde_json::from_value(Value::String(id.clone())).map_err(|e| e.to_string())?;
            if !editor
                .doc
                .node(node_id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Shape(_) | NodeKind::Frame(_)))
            {
                return Err("Fill can only be cleared on shape or frame layers".into());
            }
            let revision = update(editor, &id, |node| match &mut node.kind {
                NodeKind::Shape(shape) => shape.fill = None,
                NodeKind::Frame(frame) => frame.fill = None,
                _ => unreachable!("target type validated"),
            })?;
            json!({"revision":revision})
        }
        Operation::SetStroke { id, color, width } => {
            let stroke = color_stroke(&color, width)?;
            let node_id: libforma::NodeId =
                serde_json::from_value(Value::String(id.clone())).map_err(|e| e.to_string())?;
            if !editor
                .doc
                .node(node_id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Shape(_)))
            {
                return Err("Stroke can only be set on shape layers".into());
            }
            let revision = update(editor, &id, |node| {
                if let NodeKind::Shape(shape) = &mut node.kind {
                    shape.stroke = Some(stroke);
                }
            })?;
            json!({"revision":revision})
        }
        Operation::SetOpacity { id, opacity } => {
            if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                return Err("Opacity must be from 0 to 1".into());
            }
            let revision = update(editor, &id, |node| node.opacity = opacity)?;
            json!({"revision":revision})
        }
        Operation::SetCornerRadius { id, radius } => {
            if !radius.is_finite() || radius < 0.0 {
                return Err("Corner radius must be finite and non-negative".into());
            }
            let node_id: libforma::NodeId =
                serde_json::from_value(Value::String(id.clone())).map_err(|e| e.to_string())?;
            if !editor.doc.node(node_id).is_some_and(
                |n| matches!(&n.kind,NodeKind::Shape(s) if matches!(s.geometry,Geometry::Rect{..})),
            ) {
                return Err("Corner radius applies only to rectangle shapes".into());
            }
            let revision = update(editor, &id, |node| {
                if let NodeKind::Shape(shape) = &mut node.kind
                    && let Geometry::Rect { corner_radius, .. } = &mut shape.geometry
                {
                    *corner_radius = radius;
                }
            })?;
            json!({"revision":revision})
        }
        Operation::UpdateText { id, content } => {
            let node_id: libforma::NodeId =
                serde_json::from_value(Value::String(id.clone())).map_err(|e| e.to_string())?;
            if !editor
                .doc
                .node(node_id)
                .is_some_and(|n| matches!(n.kind, NodeKind::Text(_)))
            {
                return Err("Layer is not text".into());
            }
            let revision = update(editor, &id, |node| {
                if let NodeKind::Text(text) = &mut node.kind {
                    text.content = content;
                }
            })?;
            json!({"revision":revision})
        }
        Operation::DuplicateLayer {
            id,
            offset_x,
            offset_y,
        } => {
            if !offset_x.is_finite() || !offset_y.is_finite() {
                return Err("Duplicate offsets must be finite".into());
            }
            let source_id: libforma::NodeId =
                serde_json::from_value(Value::String(id)).map_err(|e| e.to_string())?;
            let source = editor
                .doc
                .node(source_id)
                .cloned()
                .ok_or("Layer not found")?;
            let location = editor
                .doc
                .locate(source_id)
                .ok_or("Layer location not found")?;
            let siblings = editor
                .doc
                .siblings(location.page, location.parent)
                .map_err(|e| e.to_string())?;
            let index = siblings
                .iter()
                .position(|n| n.id == source_id)
                .ok_or("Layer location not found")?
                + 1;
            let mut copy = source.duplicate();
            copy.transform = Affine::translate((offset_x, offset_y)) * copy.transform;
            let new_id = copy.id.to_string();
            if !editor.apply(Command::InsertNode {
                location: NodeLocation { index, ..location },
                node: copy,
            }) {
                return Err("Could not duplicate layer".into());
            }
            json!({"id":new_id,"revision":editor.revision})
        }
        Operation::SetVisibility { id, visible } => {
            let revision = update(editor, &id, |node| node.visible = visible)?;
            json!({"revision":revision})
        }
        Operation::Undo => {
            let before = editor.revision;
            editor.undo();
            json!({"revision":editor.revision,"changed":editor.revision!=before})
        }
        Operation::Redo => {
            let before = editor.revision;
            editor.redo();
            json!({"revision":editor.revision,"changed":editor.revision!=before})
        }
        Operation::GroupLayers { ids, name } => {
            if ids.len() < 2 {
                return Err("At least two layers are required to create a group".into());
            }
            let parsed: Vec<libforma::NodeId> = ids
                .into_iter()
                .map(|s| serde_json::from_value(Value::String(s)).map_err(|e| e.to_string()))
                .collect::<Result<_, _>>()?;
            let unique: std::collections::HashSet<_> = parsed.iter().copied().collect();
            if unique.len() != parsed.len() {
                return Err("Layer IDs must be unique".into());
            }
            let first = editor.doc.locate(parsed[0]).ok_or("Layer not found")?;
            if parsed.iter().any(|id| {
                editor
                    .doc
                    .locate(*id)
                    .is_none_or(|loc| loc.page != first.page || loc.parent != first.parent)
            }) {
                return Err("Layers must be siblings".into());
            }
            let mut indexed: Vec<_> = parsed
                .iter()
                .map(|id| {
                    let loc = editor.doc.locate(*id).unwrap();
                    (loc.index, *id)
                })
                .collect();
            indexed.sort_by_key(|item| item.0);
            let indices: Vec<_> = indexed.iter().map(|item| item.0).collect();
            let children: Vec<Node> = indexed
                .iter()
                .map(|(_, id)| editor.doc.node(*id).unwrap().clone())
                .collect();
            let group = Node::group(children).with_name(name);
            let group_id = group.id.to_string();
            let mut commands: Vec<_> = indexed
                .iter()
                .rev()
                .map(|(_, id)| Command::RemoveNode { id: *id })
                .collect();
            commands.push(Command::InsertNode {
                location: NodeLocation {
                    index: indices[0],
                    ..first
                },
                node: group,
            });
            if !editor.apply(Command::Batch(commands)) {
                return Err("Could not group layers".into());
            }
            json!({"id":group_id,"revision":editor.revision})
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

    #[test]
    fn text_shape_fill_duplicate_and_undo_are_history_commands() {
        let mut editor = Editor::new_document("MCP test", None);
        let shape = execute(
            Some(&mut editor),
            Operation::CreateShape(CreateShapeArgs {
                name: "Card".into(),
                shape: "rectangle".into(),
                x: 10.0,
                y: 20.0,
                width: 100.0,
                height: 60.0,
                corner_radius: 8.0,
                fill: Some(PaintArgs::LinearGradient {
                    start: [0.0, 0.0],
                    end: [100.0, 0.0],
                    stops: vec![
                        GradientStopArgs {
                            offset: 0.0,
                            color: "#ff0000".into(),
                        },
                        GradientStopArgs {
                            offset: 1.0,
                            color: "#0000ff".into(),
                        },
                    ],
                }),
                stroke_color: None,
                stroke_width: 0.0,
            }),
        )
        .unwrap();
        let id = shape["id"].as_str().unwrap().to_owned();
        execute(
            Some(&mut editor),
            Operation::SetOpacity {
                id: id.clone(),
                opacity: 0.5,
            },
        )
        .unwrap();
        assert_eq!(
            editor
                .doc
                .node(serde_json::from_value(Value::String(id.clone())).unwrap())
                .unwrap()
                .opacity,
            0.5
        );
        let text = execute(
            Some(&mut editor),
            Operation::CreateText(CreateTextArgs {
                name: "Title".into(),
                content: "Hello Forma".into(),
                x: 20.0,
                y: 25.0,
                font_family: "Inter".into(),
                font_size: 24.0,
                font_weight: 700,
                color: "#ffffff".into(),
                align: "center".into(),
                width: Some(120.0),
            }),
        )
        .unwrap();
        let text_id = text["id"].as_str().unwrap().to_owned();
        execute(
            Some(&mut editor),
            Operation::UpdateText {
                id: text_id.clone(),
                content: "Updated".into(),
            },
        )
        .unwrap();
        assert!(
            matches!(&editor.doc.node(serde_json::from_value(Value::String(text_id.clone())).unwrap()).unwrap().kind,NodeKind::Text(t) if t.content=="Updated")
        );
        let duplicate = execute(
            Some(&mut editor),
            Operation::DuplicateLayer {
                id: id.clone(),
                offset_x: 12.0,
                offset_y: 3.0,
            },
        )
        .unwrap();
        assert_ne!(duplicate["id"], id);
        editor.undo();
        assert!(
            editor
                .doc
                .node(
                    serde_json::from_value(Value::String(
                        duplicate["id"].as_str().unwrap().to_owned()
                    ))
                    .unwrap()
                )
                .is_none()
        );
    }

    #[test]
    fn invalid_paint_or_wrong_target_does_not_change_document() {
        let mut editor = Editor::new_document("MCP test", None);
        let text = execute(
            Some(&mut editor),
            Operation::CreateText(CreateTextArgs {
                name: "Text".into(),
                content: "hello".into(),
                x: 0.0,
                y: 0.0,
                font_family: "Inter".into(),
                font_size: 16.0,
                font_weight: 400,
                color: "#000000".into(),
                align: "start".into(),
                width: None,
            }),
        )
        .unwrap();
        let id = text["id"].as_str().unwrap().to_owned();
        let revision = editor.revision;
        let bad = execute(
            Some(&mut editor),
            Operation::SetFill {
                id: id.clone(),
                fill: PaintArgs::Solid {
                    color: "not-a-color".into(),
                },
            },
        );
        assert!(bad.is_err());
        assert_eq!(editor.revision, revision);
        assert_eq!(editor.current_page().children.len(), 1);
        let wrong_kind = execute(
            Some(&mut editor),
            Operation::SetFill {
                id,
                fill: PaintArgs::Solid {
                    color: "#ffffff".into(),
                },
            },
        );
        assert!(wrong_kind.is_err());
        assert_eq!(editor.revision, revision);
    }

    #[test]
    fn groups_consecutive_siblings_and_can_undo() {
        let mut editor = Editor::new_document("MCP test", None);
        let mut ids = Vec::new();
        for name in ["A", "B"] {
            let v = execute(
                Some(&mut editor),
                Operation::CreateRectangle {
                    name: name.into(),
                    x: 0.0,
                    y: 0.0,
                    width: 10.0,
                    height: 10.0,
                },
            )
            .unwrap();
            ids.push(v["id"].as_str().unwrap().to_owned());
        }
        let group = execute(
            Some(&mut editor),
            Operation::GroupLayers {
                ids,
                name: "Group".into(),
            },
        )
        .unwrap();
        let group_id: libforma::NodeId =
            serde_json::from_value(Value::String(group["id"].as_str().unwrap().to_owned()))
                .unwrap();
        assert_eq!(editor.doc.node(group_id).unwrap().children().len(), 2);
        editor.undo();
        assert_eq!(editor.current_page().children.len(), 2);
    }
}
