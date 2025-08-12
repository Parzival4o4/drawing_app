import { logout } from "../api.js";
import { navigateTo } from "../router.js";

export function renderHome() {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <h2>Home</h2>
    <p>Welcome! You are logged in.</p>
    <button id="logout-btn">Logout</button>
  `;

  document.getElementById("logout-btn")?.addEventListener("click", async () => {
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
}
