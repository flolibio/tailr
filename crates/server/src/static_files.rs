use axum::response::Html;
use axum::routing::get;
use axum::Router;

pub fn routes() -> Router {
    Router::new().fallback(get(serve_frontend))
}

async fn serve_frontend() -> Html<String> {
    Html(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>LogViewer</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body { font-family: monospace; background: #1a1a2e; color: #e0e0e0; }
        .header { background: #16213e; padding: 12px 20px; display: flex; align-items: center; gap: 16px; border-bottom: 1px solid #0f3460; }
        .header h1 { font-size: 18px; color: #e94560; }
        .container { display: flex; height: calc(100vh - 48px); }
        .sidebar { width: 280px; background: #16213e; border-right: 1px solid #0f3460; overflow-y: auto; padding: 8px; }
        .sidebar .file { padding: 6px 10px; cursor: pointer; border-radius: 4px; font-size: 13px; }
        .sidebar .file:hover { background: #0f3460; }
        .sidebar .file.active { background: #e94560; color: white; }
        .main { flex: 1; display: flex; flex-direction: column; }
        .toolbar { padding: 8px 16px; background: #16213e; border-bottom: 1px solid #0f3460; display: flex; gap: 8px; align-items: center; }
        .toolbar input { background: #1a1a2e; border: 1px solid #0f3460; color: #e0e0e0; padding: 6px 10px; border-radius: 4px; font-family: monospace; font-size: 13px; }
        .toolbar input:focus { outline: none; border-color: #e94560; }
        .log-area { flex: 1; overflow-y: auto; padding: 8px 16px; font-size: 13px; line-height: 1.6; }
        .log-line { white-space: pre-wrap; word-break: break-all; padding: 1px 0; }
        .log-line .line-num { color: #555; display: inline-block; width: 60px; text-align: right; margin-right: 12px; user-select: none; }
        .level-ERROR { color: #ff4444; }
        .level-WARN { color: #ffbb33; }
        .level-INFO { color: #00C851; }
        .level-DEBUG { color: #33b5e5; }
        .level-TRACE { color: #aa66cc; }
        .status { padding: 4px 16px; background: #16213e; border-top: 1px solid #0f3460; font-size: 12px; color: #888; }
    </style>
</head>
<body>
    <div class="header">
        <h1>LogViewer</h1>
        <span id="status" style="font-size:12px;color:#888">Connecting...</span>
    </div>
    <div class="container">
        <div class="sidebar" id="files">Loading files...</div>
        <div class="main">
            <div class="toolbar">
                <input type="text" id="search" placeholder="Search (regex supported)..." style="flex:1" />
                <input type="text" id="path-input" placeholder="Log directory path..." style="width:300px" />
                <button onclick="loadFiles()" style="background:#e94560;color:white;border:none;padding:6px 12px;border-radius:4px;cursor:pointer;font-family:monospace">Load</button>
            </div>
            <div class="log-area" id="logs"><div style="padding:20px;color:#888">Select a file from the sidebar to view logs</div></div>
        </div>
    </div>
    <div class="status" id="bottom-status">Ready</div>
    <script>
        let ws = null;
        let activeFile = null;
        let entries = [];

        async function loadFiles(path) {
            const url = path ? '/api/files?path=' + encodeURIComponent(path) : '/api/files';
            const res = await fetch(url);
            const data = await res.json();
            const sidebar = document.getElementById('files');
            if (!data.success) { sidebar.innerHTML = '<div style="color:red;padding:8px">' + data.error + '</div>'; return; }
            sidebar.innerHTML = '';
            data.data.entries.forEach(e => {
                const div = document.createElement('div');
                div.className = 'file';
                div.textContent = (e.isDir ? '[DIR] ' : '') + e.name;
                div.onclick = () => selectFile(e.path, e.isDir);
                sidebar.appendChild(div);
            });
        }

        async function selectFile(path, isDir) {
            if (isDir) {
                document.getElementById('path-input').value = path;
                loadFiles(path);
                return;
            }
            document.querySelectorAll('.file').forEach(f => f.classList.remove('active'));
            event.target.classList.add('active');
            activeFile = path;

            if (ws && ws.readyState === WebSocket.OPEN) {
                ws.send(JSON.stringify({ type: 'unsubscribe', path: activeFile }));
            }

            const res = await fetch('/api/file/content?path=' + encodeURIComponent(path) + '&limit=500');
            const data = await res.json();
            if (!data.success) return;

            entries = data.data.entries;
            renderEntries();

            connectWS(path);
        }

        function connectWS(path) {
            if (ws) ws.close();
            ws = new WebSocket('ws://' + location.host + '/ws');
            ws.onopen = () => {
                document.getElementById('status').textContent = 'Connected';
                ws.send(JSON.stringify({ type: 'subscribe', path: path, afterSeq: null }));
            };
            ws.onmessage = (event) => {
                const msg = JSON.parse(event.data);
                if (msg.type === 'append' && msg.path === activeFile) {
                    entries.push(...msg.entries);
                    const logArea = document.getElementById('logs');
                    const wasAtBottom = logArea.scrollHeight - logArea.scrollTop - logArea.clientHeight < 50;
                    msg.entries.forEach(e => appendEntryDOM(e));
                    if (wasAtBottom) logArea.scrollTop = logArea.scrollHeight;
                    document.getElementById('bottom-status').textContent = entries.length + ' lines';
                }
            };
            ws.onclose = () => { document.getElementById('status').textContent = 'Disconnected'; };
        }

        function renderEntries() {
            const logArea = document.getElementById('logs');
            logArea.innerHTML = '';
            entries.forEach(e => appendEntryDOM(e));
            logArea.scrollTop = logArea.scrollHeight;
            document.getElementById('bottom-status').textContent = entries.length + ' lines';
        }

        function appendEntryDOM(e) {
            const logArea = document.getElementById('logs');
            const div = document.createElement('div');
            div.className = 'log-line';
            const levelClass = 'level-' + e.level;
            div.innerHTML = '<span class="line-num">' + e.lineNum + '</span><span class="' + levelClass + '">' + escapeHtml(e.raw) + '</span>';
            logArea.appendChild(div);
        }

        function escapeHtml(s) {
            return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
        }

        loadFiles();
    </script>
</body>
</html>"#
        .to_string(),
    )
}
