import { logout } from "../api.js";
import { navigateTo } from "../router.js";

let ws: WebSocket | null = null;

export function renderCanvasDebugPage(id: string) {
  const app = document.getElementById("app")!;
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

  const connectBtn = document.getElementById("connect-btn") as HTMLButtonElement;
  const sendBtn = document.getElementById("send-msg-btn") as HTMLButtonElement;
  const logoutBtn = document.getElementById("logout-btn") as HTMLButtonElement;
  const messagesPre = document.getElementById("messages") as HTMLPreElement;

  const logMessage = (msg: string) => {
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
    } else {
      logMessage("WebSocket is not open. Please connect first.");
    }
  });

  logoutBtn.addEventListener("click", async () => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.close();
    }

    try {
      const res = await logout();
      if (res.ok) {
        navigateTo("/login");
      } else {
        alert("Logout failed");
      }
    } catch {
      alert("Network error");
    }
  });

  document.getElementById("link-home")?.addEventListener("click", (e) => {
    e.preventDefault();
    navigateTo("/");
  });
}
