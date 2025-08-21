// src/pages/drawer/BackendSync.ts
export class BackendSync {
    es;
    canvasId;
    socket;
    constructor(es, canvasId) {
        this.es = es;
        this.canvasId = canvasId;
        const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
        const host = window.location.host;
        const url = `${protocol}//${host}/ws`;
        this.socket = new WebSocket(url);
        this.socket.addEventListener("open", () => {
            console.log("[BackendSync] Connected to backend:", url);
            // Send registration message
            const registerMsg = {
                command: "registerForCanvas",
                canvasId: this.canvasId,
            };
            this.socket.send(JSON.stringify(registerMsg));
            console.log("[BackendSync] Sent registration:", registerMsg);
        });
        this.socket.addEventListener("message", (event) => this.handleIncomingMessage(event.data));
        this.socket.addEventListener("close", () => {
            console.warn("[BackendSync] Connection closed");
        });
        this.socket.addEventListener("error", (err) => {
            console.error("[BackendSync] Socket error:", err);
        });
        // Forward local events to backend
        this.es.register((event) => {
            this.send(event);
        });
    }
    handleIncomingMessage(data) {
        try {
            const parsed = JSON.parse(data);
            console.log("[BackendSync] Incoming message:", parsed);
            if (parsed.canvasId === this.canvasId &&
                Array.isArray(parsed.eventsForCanvas)) {
                parsed.eventsForCanvas.forEach((ev) => {
                    this.es.apply(ev);
                });
            }
        }
        catch (err) {
            console.error("[BackendSync] Failed to parse message", err, data);
        }
    }
    send(event) {
        if (this.socket.readyState === WebSocket.OPEN) {
            const message = {
                canvasId: this.canvasId,
                eventsForCanvas: [event],
            };
            this.socket.send(JSON.stringify(message));
        }
        else {
            console.warn("[BackendSync] Tried to send while socket not open", event);
        }
    }
}
//# sourceMappingURL=BackendSync.js.map