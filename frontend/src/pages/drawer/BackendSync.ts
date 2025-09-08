import type { EventSystem, Canvas } from "./drawer.js";

type Handlers = {
  setEditingPower?: (canEdit: boolean) => void;
  setModerationState?: (isModerated: boolean) => void;
  setModerationPower?: (canToggleModeration: boolean) => void;
};

export class BackendSync {
  private socket: WebSocket;
  private handlers: Handlers = {};

  // track current backend state
  private moderationState: boolean = false;
  private userPermission: string | null = null;

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

    this.socket.addEventListener("message", (evt) =>
      this.handleIncomingMessage(evt.data)
    );
    this.socket.addEventListener("close", () =>
      console.warn("[BackendSync] Connection closed")
    );
    this.socket.addEventListener("error", (err) =>
      console.error("[BackendSync] Socket error:", err)
    );

    // Forward local events to backend only (do NOT apply here)
    this.es.register((event: any) => this.send(event));
  }

  /**
   * Public method to set the UI handlers after the instance is created.
   * @param handlers
   */
  public setHandlers(handlers: Handlers) {
    this.handlers = handlers;
  }

  public sendToggleModeratedCommand() {
    if (this.socket.readyState !== WebSocket.OPEN) {
      console.warn("[BackendSync] Tried to send toggle command while socket not open.");
      return;
    }

    const commandMessage = {
      canvasId: this.canvasId,
      command: "toggleModerated"
    };
    this.socket.send(JSON.stringify(commandMessage));
    console.log("[BackendSync] Sent moderation toggle command.");
  }

  private handleIncomingMessage(data: string) {
    try {
      console.log("[BackendSync] Incoming plain:", data);
      const msg = JSON.parse(data);

      if (msg.canvasId !== this.canvasId) return;

      // Moderation state messages
      if (typeof msg.moderated === "boolean") {
        this.moderationState = msg.moderated;
        this.handlers.setModerationState?.(msg.moderated);
        this.updateEditingPower(); // recalc based on new moderation state
        return;
      }

      // Permission messages
      if (typeof msg.yourPermission === "string") {
        this.userPermission = msg.yourPermission;

        // Owner, Co-owner or Moderator can toggle moderation
        const canToggleModeration =
          this.userPermission === "O" ||
          this.userPermission === "M" ||
          this.userPermission === "C";
        this.handlers.setModerationPower?.(canToggleModeration);

        this.updateEditingPower(); // recalc based on new permission
        return;
      }

      // History / event replay messages
      if (Array.isArray(msg.eventsForCanvas)) {
        msg.eventsForCanvas.forEach((ev: any) => {
          this.canvas.apply(ev);
        });
        return;
      }
    } catch (err) {
      console.error("[BackendSync] Failed to parse message", err, data);
    }
  }

  /**
   * Decide if user can edit given current permission + moderation state.
   */
  private updateEditingPower() {
    if (!this.userPermission) return;

    let canEdit = false;
    const perm = this.userPermission;

    if (["C", "O", "M", "V"].includes(perm)) {
      // Co-owner, Owner, Moderator, VIP can always edit
      canEdit = true;
    } else if (perm === "W") {
      // Writer can only edit if moderation is OFF
      canEdit = !this.moderationState;
    } else {
      // R (read-only) or unknown → no editing
      canEdit = false;
    }

    this.handlers.setEditingPower?.(canEdit);
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