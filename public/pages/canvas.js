var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
import { logout } from "../api.js";
import { navigateTo } from "../router.js";
let ws = null;
export function renderCanvasDebugPage(id) {
    var _a;
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>WebSocket Debug Page</h2>
    <p>Testing canvas session with ID: <strong>${id}</strong></p>
    <div style="display: flex; gap: 10px; margin-bottom: 20px;">
      <button id="connect-btn">Connect to WebSocket</button>
      <button id="send-msg-btn" disabled>Send Test Message</button>
    </div>
    <button id="logout-btn">Logout</button>
    <h3>Server Messages:</h3>
    <pre id="messages"></pre>
    <p><a href="/" id="link-home">← Back to Home</a></p>
  `;
    const connectBtn = document.getElementById("connect-btn");
    const sendBtn = document.getElementById("send-msg-btn");
    const logoutBtn = document.getElementById("logout-btn");
    const messagesPre = document.getElementById("messages");
    const logMessage = (msg) => {
        messagesPre.textContent += `${msg}\n`;
        messagesPre.scrollTop = messagesPre.scrollHeight;
    };
    const connectToWebSocket = () => {
        if (ws && ws.readyState === WebSocket.OPEN) {
            logMessage("WebSocket is already connected.");
            return;
        }
        const wsUrl = `ws://${window.location.host}/ws`;
        logMessage(`Attempting to connect to ${wsUrl}...`);
        ws = new WebSocket(wsUrl);
        ws.onopen = () => {
            logMessage("Connection established!");
            sendBtn.disabled = false;
            connectBtn.disabled = true;
        };
        ws.onmessage = (event) => {
            logMessage(`Received: ${event.data}`);
        };
        ws.onclose = (event) => {
            logMessage(`Connection closed. Code: ${event.code}, Reason: ${event.reason}`);
            sendBtn.disabled = true;
            connectBtn.disabled = false;
            ws = null;
        };
        ws.onerror = (error) => {
            console.error("WebSocket error:", error);
            logMessage("WebSocket error occurred. Check console for details.");
            sendBtn.disabled = true;
            connectBtn.disabled = false;
        };
    };
    connectBtn.addEventListener("click", connectToWebSocket);
    sendBtn.addEventListener("click", () => {
        if (ws && ws.readyState === WebSocket.OPEN) {
            const message = "hello backend";
            ws.send(message);
            logMessage(`Sent: ${message}`);
        }
        else {
            logMessage("WebSocket is not open. Please connect first.");
        }
    });
    logoutBtn.addEventListener("click", () => __awaiter(this, void 0, void 0, function* () {
        if (ws && ws.readyState === WebSocket.OPEN) {
            ws.close();
        }
        try {
            const res = yield logout();
            if (res.ok) {
                navigateTo("/login");
            }
            else {
                alert("Logout failed");
            }
        }
        catch (_a) {
            alert("Network error");
        }
    }));
    (_a = document.getElementById("link-home")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", (e) => {
        e.preventDefault();
        navigateTo("/");
    });
}
//# sourceMappingURL=canvas.js.map