// src/pages/canvas.ts
import { navigateTo } from "../router.js";
export function renderCanvasPage(canvasId) {
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>Canvas</h2>
    <div style="display: flex; gap: 20px;">
      <!-- Tools sidebar -->
      <div style="flex: 0 0 200px; border-right: 1px solid #ccc; padding-right: 10px;">
        <button id="home-btn" class="nav-btn">🏠 Home</button>
        <h3>Tools</h3>
        <div class="tools"></div>
      </div>

      <!-- Drawing area -->
      <div style="flex: 1; padding-left: 10px;">
        <canvas id="drawArea" width="1024" height="768"></canvas>

        <!-- Hidden UI expected by drawer.ts -->
        <textarea id="textarea" cols="130" rows="20" name="event_log" style="display:none;"></textarea>
        <button id="button" type="button" style="display:none;">Load</button>
      </div>
    </div>
  `;
    // Navigate back home
    document.getElementById("home-btn")?.addEventListener("click", () => {
        navigateTo("/");
    });
    // Load drawer setup
    import("./drawer/drawer.js")
        .then((mod) => {
        if (typeof mod.setupDrawer === "function") {
            const canvasElm = document.getElementById("drawArea");
            const toolsElm = document.querySelector(".tools");
            const textAreaElm = document.getElementById("textarea");
            const buttonElm = document.getElementById("button");
            mod.setupDrawer(canvasElm, toolsElm, textAreaElm, buttonElm);
        }
    })
        .catch((err) => {
        console.error("Failed to load drawer:", err);
    });
}
//# sourceMappingURL=canvas.js.map