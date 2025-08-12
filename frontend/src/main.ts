// API base URLs
const API_BASE = "/api";

document.addEventListener("DOMContentLoaded", () => {
  handleRoute();
  window.addEventListener("popstate", handleRoute);
});

async function handleRoute() {
  const path = window.location.pathname;

  if (path === "/login") {
    renderLoginPage();
  } else if (path === "/register") {
    renderRegisterPage();
  } else if (path === "/") {
    const auth = await isAuthenticated();
    if (auth) {
      renderHome();
    } else {
      navigateTo("/login");
    }
  } else {
    // Unknown route — redirect to home or login
    navigateTo("/");
  }
}

async function isAuthenticated(): Promise<boolean> {
  try {
    const res = await fetch(`${API_BASE}/me`, {
      credentials: "include",
    });
    return res.ok;
  } catch {
    return false;
  }
}

function navigateTo(path: string) {
  history.pushState(null, "", path);
  handleRoute();
}

function renderLoginPage() {
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
    <p id="login-error" style="color:red;"></p>
    <p>Don't have an account? <a href="/register" id="link-register">Register here</a></p>
  `;

  document.getElementById("link-register")?.addEventListener("click", (e) => {
    e.preventDefault();
    navigateTo("/register");
  });

  const form = document.getElementById("login-form") as HTMLFormElement;
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const email = (document.getElementById("email") as HTMLInputElement).value;
    const password = (document.getElementById("password") as HTMLInputElement).value;
    const errorEl = document.getElementById("login-error")!;

    try {
      const res = await fetch(`${API_BASE}/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ email, password }),
      });

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

function renderRegisterPage() {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <h2>Register</h2>
    <form id="register-form">
      <input type="text" id="displayName" placeholder="Display Name" required />
      <br />
      <input type="email" id="reg-email" placeholder="Email" required />
      <br />
      <input type="password" id="reg-password" placeholder="Password" required />
      <br />
      <button type="submit">Sign Up</button>
    </form>
    <p id="register-error" style="color:red;"></p>
    <p>Already have an account? <a href="/login" id="link-login">Login here</a></p>
  `;

  document.getElementById("link-login")?.addEventListener("click", (e) => {
    e.preventDefault();
    navigateTo("/login");
  });

  const form = document.getElementById("register-form") as HTMLFormElement;
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const displayName = (document.getElementById("displayName") as HTMLInputElement).value;
    const email = (document.getElementById("reg-email") as HTMLInputElement).value;
    const password = (document.getElementById("reg-password") as HTMLInputElement).value;
    const errorEl = document.getElementById("register-error")!;

    try {
      const res = await fetch(`${API_BASE}/register`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password, displayName }),
      });

      if (res.ok) {
        navigateTo("/login");
      } else {
        const errText = await res.text();
        errorEl.textContent = errText || "Registration failed";
      }
    } catch (err) {
      console.error("Register error:", err);
      errorEl.textContent = "Network error";
    }
  });
}

function renderHome() {
  const app = document.getElementById("app")!;
  app.innerHTML = `
    <h2>Home</h2>
    <p>Welcome! You are logged in.</p>
    <button id="logout-btn">Logout</button>
  `;

  document.getElementById("logout-btn")?.addEventListener("click", async () => {
    try {
      const res = await fetch(`${API_BASE}/logout`, {
        method: "POST",
        credentials: "include",
      });
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
