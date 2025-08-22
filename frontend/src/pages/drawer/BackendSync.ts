// src/pages/drawer/BackendSync.ts
import type { EventSystem, Canvas } from "./drawer.js";

export class BackendSync {
  private socket: WebSocket;

  constructor(
    private es: EventSystem,
    private canvas: Canvas,
    private canvasId: string
  ) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const host = window.location.host;
    const url = `${protocol}//${host}/ws`;

    this.socket = new WebSocket(url);

    this.socket.addEventListener("open", () => {
      const registerMsg = { command: "registerForCanvas", canvasId: this.canvasId };
      this.socket.send(JSON.stringify(registerMsg));
      console.log("[BackendSync] Connected & registered:", registerMsg);
    });

    this.socket.addEventListener("message", (evt) => this.handleIncomingMessage(evt.data));
    this.socket.addEventListener("close", () => console.warn("[BackendSync] Connection closed"));
    this.socket.addEventListener("error", (err) => console.error("[BackendSync] Socket error:", err));

    // Forward local events to backend only (do NOT apply here)
    this.es.register((event: any) => this.send(event));
  }

  private handleIncomingMessage(data: string) {
    try {
      console.log("[BackendSync] Incoming plain:", data);
      const msg = JSON.parse(data);
      console.log("[BackendSync] Incoming parsed:", msg);

      if (msg.canvasId !== this.canvasId) return;
      if (!Array.isArray(msg.eventsForCanvas)) return;

      // Apply events directly to canvas to avoid echo loop
      msg.eventsForCanvas.forEach((ev: any) => {
        this.canvas.apply(ev);
      });
    } catch (err) {
      console.error("[BackendSync] Failed to parse message", err, data);
    }
  }

  private send(event: any) {
    if (this.socket.readyState !== WebSocket.OPEN) {
      console.warn("[BackendSync] Tried to send while socket not open", event);
      return;
    }
    const message = {
      canvasId: this.canvasId,
      eventsForCanvas: [event],
    };
    this.socket.send(JSON.stringify(message));
  }
}
