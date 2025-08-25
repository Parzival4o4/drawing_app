import { createCanvas, getCanvases, getUserInfo, logout, updateUserInfo } from "../api.js";
import { navigateTo } from "../router.js";
// === Helper to map permissions ===
function formatPermission(p) {
    switch (p) {
        case "R": return { label: "Read", color: "gray" };
        case "W": return { label: "Write", color: "blue" };
        case "V": return { label: "Write+", color: "dodgerblue" };
        case "M": return { label: "Moderator", color: "orange" };
        case "O": return { label: "Owned", color: "green" };
        case "C": return { label: "Co-Owned", color: "teal" };
        default: return { label: "Unknown", color: "black" };
    }
}
export function renderHome() {
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>Home</h2>
    <div style="display: flex; gap: 20px;">
      <!-- Left column: canvases -->
      <div style="flex: 1; border-right: 1px solid #ccc; padding-right: 10px;">
        <h3>Your Canvases</h3>
        <ul id="canvas-list" style="list-style: none; padding: 0;"></ul>
      </div>

      <!-- Right column: user options -->
      <div style="flex: 1; padding-left: 10px;">
        <h3>User Options</h3>

        <!-- User Info -->
        <section class="home-section">
          <h4>Your Info</h4>
          <div><b>User ID:</b> <span id="user-id"></span></div>
        </section>

        <!-- Create new canvas -->
        <section class="home-section">
          <h4>Create New Canvas</h4>
          <input id="new-canvas-name" type="text" placeholder="Canvas name" />
          <button id="create-canvas-btn">Create</button>
          <div id="create-canvas-msg" style="font-size: 0.9em; margin-top: 5px;"></div>
        </section>

        <!-- Update user info -->
        <section class="home-section">
          <h4>Update User Info</h4>
          <div style="margin-bottom: 8px;">
            <label for="user-email">Email:</label>
            <input id="user-email" type="email" placeholder="Email" />
          </div>
          <div style="margin-bottom: 8px;">
            <label for="user-display">Display Name:</label>
            <input id="user-display" type="text" placeholder="Display name" />
          </div>
          <button id="update-user-btn">Save</button>
          <div id="update-user-msg" style="font-size: 0.9em; margin-top: 5px;"></div>
        </section>

        <!-- Logout -->
        <section class="home-section">
          <button id="logout-btn">Logout</button>
        </section>
      </div>
    </div>
  `;
    const canvasList = document.getElementById("canvas-list");
    const logoutBtn = document.getElementById("logout-btn");
    const createBtn = document.getElementById("create-canvas-btn");
    const createInput = document.getElementById("new-canvas-name");
    const createMsg = document.getElementById("create-canvas-msg");
    const updateBtn = document.getElementById("update-user-btn");
    const updateEmail = document.getElementById("user-email");
    const updateDisplay = document.getElementById("user-display");
    const updateMsg = document.getElementById("update-user-msg");
    // === Fetch canvases from backend ===
    const loadCanvases = async () => {
        try {
            const res = await getCanvases();
            if (!res.ok) {
                canvasList.innerHTML = `<li>Failed to load canvases.</li>`;
                return;
            }
            const canvases = await res.json();
            canvasList.innerHTML = canvases.length
                ? ""
                : `<li>No canvases available.</li>`;
            canvases.forEach((c) => {
                const { label, color } = formatPermission(c.permission_level);
                const li = document.createElement("li");
                li.style.cursor = "pointer";
                li.style.padding = "5px 0";
                li.innerHTML = `
          ${c.name} <span style="
            background-color: ${color};
            color: white;
            padding: 2px 6px;
            border-radius: 6px;
            font-size: 0.85em;
            margin-left: 6px;
          ">${label}</span>
        `;
                li.addEventListener("click", () => navigateTo(`/canvas/${c.canvas_id}`));
                canvasList.appendChild(li);
            });
        }
        catch (err) {
            console.error(err);
            canvasList.innerHTML = `<li>Network error while loading canvases.</li>`;
        }
    };
    loadCanvases();
    // === Prefill user info ===
    const loadUserInfo = async () => {
        try {
            const user = await getUserInfo();
            updateEmail.value = user.email;
            updateDisplay.value = user.display_name;
            // ✅ Display user_id
            const userIdSpan = document.getElementById("user-id");
            userIdSpan.textContent = user.user_id;
        }
        catch (err) {
            console.error(err);
        }
    };
    loadUserInfo();
    // === Logout ===
    logoutBtn.addEventListener("click", async () => {
        try {
            const res = await logout();
            if (res.ok)
                navigateTo("/login");
            else
                alert("Logout failed");
        }
        catch {
            alert("Network error");
        }
    });
    // === Create new canvas ===
    createBtn.addEventListener("click", async () => {
        const name = createInput.value.trim();
        if (!name) {
            createMsg.style.color = "red";
            createMsg.textContent = "Canvas name required.";
            return;
        }
        try {
            const res = await createCanvas(name);
            if (res.ok) {
                createMsg.style.color = "green";
                createMsg.textContent = "Canvas created!";
                createInput.value = "";
                loadCanvases();
            }
            else {
                const err = await res.text();
                createMsg.style.color = "red";
                createMsg.textContent = `Failed: ${err}`;
            }
        }
        catch {
            createMsg.style.color = "red";
            createMsg.textContent = "Network error.";
        }
    });
    // === Update user info ===
    updateBtn.addEventListener("click", async () => {
        const email = updateEmail.value.trim();
        const display_name = updateDisplay.value.trim();
        if (!email && !display_name) {
            updateMsg.style.color = "red";
            updateMsg.textContent = "Please provide at least one field.";
            return;
        }
        try {
            const res = await updateUserInfo(email, display_name);
            if (res.ok) {
                updateMsg.style.color = "green";
                updateMsg.textContent = "User info updated!";
            }
            else {
                const err = await res.text();
                updateMsg.style.color = "red";
                updateMsg.textContent = `Failed: ${err}`;
            }
        }
        catch {
            updateMsg.style.color = "red";
            updateMsg.textContent = "Network error.";
        }
    });
}
//# sourceMappingURL=home.js.map