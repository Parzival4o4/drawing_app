import { navigateTo } from "../router.js";

export function renderCanvasPage(canvasId: string, userId: string) {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <h2>Canvas</h2>
    <div style="display: flex; gap: 20px;">
      <div style="flex: 0 0 260px; border-right: 1px solid #ccc; padding-right: 10px;">
        <button id="home-btn" class="nav-btn">🏠 Home</button>

        <h3>Tools</h3>
        <div class="tools"></div>

        <h3 style="margin-top: 20px;">Moderation</h3>
        <div id="moderation-container" style="font-size: 0.9em;"></div>

        <h3 style="margin-top: 20px;">Permissions</h3>
        <div id="permissions-container" style="font-size: 0.9em;"></div>
      </div>

      <div style="flex: 1; padding-left: 10px;">
        <canvas id="drawArea" width="1024" height="768" style="border:1px solid #ccc;"></canvas>
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
        const canvasElm = document.getElementById("drawArea") as HTMLCanvasElement;
        const toolsElm = document.querySelector(".tools") as HTMLElement;
        const moderationElm = document.getElementById("moderation-container") as HTMLElement;

        mod.setupDrawer(canvasElm, toolsElm,  moderationElm, canvasId, userId);
      }
    })
    .catch((err) => {
      console.error("Failed to load drawer:", err);
    });

  // === Permissions UI ===
  loadPermissions(canvasId, userId);
}


async function loadPermissions(canvasId: string, currentUserId: string) {
  const container = document.getElementById("permissions-container")!;
  container.innerHTML = "Loading...";

  try {
    const res = await fetch(`/api/canvas/${canvasId}/permissions`);
    if (!res.ok) {
      container.innerHTML = `<div style="color:red;">Failed to load permissions.</div>`;
      return;
    }

    const perms = await res.json();
    container.innerHTML = "";

    const permLabels: Record<string, string> = {
      "R": "Read",
      "W": "Write",
      "V": "Write+",
      "M": "Moderator",
      "O": "Owner",
      "C": "Co-Owner"
    };

    // === Add User Form (always adds to Read) ===
    const addUserSection = document.createElement("div");
    addUserSection.style.marginBottom = "15px";
    addUserSection.innerHTML = `
      <h4>Add User</h4>
      <input type="text" placeholder="User ID" style="width:120px;" />
      <button>Add to Read</button>
    `;
    const input = addUserSection.querySelector("input") as HTMLInputElement;
    const btn = addUserSection.querySelector("button") as HTMLButtonElement;
    btn.addEventListener("click", async () => {
      const targetId = input.value.trim();
      if (!targetId) return;
      await updatePermission(canvasId, parseInt(targetId, 10), "R");
      input.value = "";
      await loadPermissions(canvasId, currentUserId);
    });
    container.appendChild(addUserSection);

    // === Permission Sections ===
    Object.keys(permLabels).forEach((perm) => {
      const users = perms[perm] || [];
      const section = document.createElement("div");
      section.style.marginBottom = "15px";
      section.style.border = "1px solid #ccc";
      section.style.padding = "8px";
      section.style.borderRadius = "6px";

      const title = document.createElement("h4");
      title.textContent = permLabels[perm];
      title.style.margin = "6px 0";
      section.appendChild(title);

      if (users.length === 0) {
        section.innerHTML += `<div style="color:#999; margin-left:8px; font-style:italic;">No users with this permission</div>`;
      } else {
        const ul = document.createElement("ul");
        ul.style.paddingLeft = "18px";
        users.forEach((u: any) => {
          const li = document.createElement("li");
          if (u.user_id === currentUserId) {
            li.innerHTML = `<strong>${u.display_name} (${u.user_id})</strong> ← You`;
            li.style.color = "green";
          } else {
            li.textContent = `${u.display_name} (${u.user_id})`;
          }

          // === Only allow changes if not Owner and not self ===
          if (perm !== "O" && u.user_id !== currentUserId) {
            const controls = document.createElement("span");
            controls.style.marginLeft = "10px";

            // Move dropdown
            const select = document.createElement("select");
            Object.keys(permLabels).forEach((p) => {
              const opt = document.createElement("option");
              opt.value = p;
              opt.textContent = permLabels[p];
              if (p === perm) opt.selected = true;
              select.appendChild(opt);
            });
            select.addEventListener("change", async () => {
              await updatePermission(canvasId, u.user_id, select.value);
              await loadPermissions(canvasId, currentUserId);
            });
            controls.appendChild(select);

            // Remove button
            const removeBtn = document.createElement("button");
            removeBtn.textContent = "❌";
            removeBtn.style.marginLeft = "6px";
            removeBtn.addEventListener("click", async () => {
              await updatePermission(canvasId, u.user_id, "");
              await loadPermissions(canvasId, currentUserId);
            });
            controls.appendChild(removeBtn);

            li.appendChild(controls);
          }

          ul.appendChild(li);
        });
        section.appendChild(ul);
      }

      container.appendChild(section);
    });
  } catch (err) {
    console.error("Failed to load permissions:", err);
    container.innerHTML = `<div style="color:red;">Network error.</div>`;
  }
}

async function updatePermission(canvasId: string, targetUserId: number, newPerm: string) {
  try {
    const res = await fetch(`/api/canvas/${canvasId}/permissions`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ user_id: targetUserId, permission: newPerm }),
    });

    if (!res.ok) {
      const err = await res.text();
      alert(`Failed to update permission: ${err}`);
    }
  } catch (err) {
    console.error("Error updating permission:", err);
    alert("Network error");
  }
}
