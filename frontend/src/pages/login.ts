import { login } from "../api.js";
import { navigateTo } from "../router.js";

export function renderLoginPage() {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <h2>Login</h2>
    <form id="login-form">
      <input type="email" id="email" placeholder="Email" required />
      <br />
      <input type="password" id="password" placeholder="Password" required />
      <br />
      <button type="submit">Login</button>
    </form>
    <p id="login-error"></p>
    <p>Don't have an account? <a href="/register" id="link-register">Register here</a></p>
  `;

  document.getElementById("link-register")?.addEventListener("click", (e) => {
    e.preventDefault();
    navigateTo("/register");
  });

  document.getElementById("login-form")?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const email = (document.getElementById("email") as HTMLInputElement).value;
    const password = (document.getElementById("password") as HTMLInputElement).value;
    const errorEl = document.getElementById("login-error")!;

    try {
      const res = await login(email, password);
      if (res.ok) {
        navigateTo("/");
      } else {
        errorEl.textContent = "Invalid email or password";
      }
    } catch (err) {
      console.error("Login error:", err);
      errorEl.textContent = "Network error";
    }
  });
}
