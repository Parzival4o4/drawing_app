var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
// API base URLs
const API_BASE = "/api";
document.addEventListener("DOMContentLoaded", () => {
    handleRoute();
    window.addEventListener("popstate", handleRoute);
});
function handleRoute() {
    return __awaiter(this, void 0, void 0, function* () {
        const path = window.location.pathname;
        if (path === "/login") {
            renderLoginPage();
        }
        else if (path === "/register") {
            renderRegisterPage();
        }
        else if (path === "/") {
            const auth = yield isAuthenticated();
            if (auth) {
                renderHome();
            }
            else {
                navigateTo("/login");
            }
        }
        else {
            // Unknown route — redirect to home or login
            navigateTo("/");
        }
    });
}
function isAuthenticated() {
    return __awaiter(this, void 0, void 0, function* () {
        try {
            const res = yield fetch(`${API_BASE}/me`, {
                credentials: "include",
            });
            return res.ok;
        }
        catch (_a) {
            return false;
        }
    });
}
function navigateTo(path) {
    history.pushState(null, "", path);
    handleRoute();
}
function renderLoginPage() {
    var _a;
    const app = document.getElementById("app");
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
    (_a = document.getElementById("link-register")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", (e) => {
        e.preventDefault();
        navigateTo("/register");
    });
    const form = document.getElementById("login-form");
    form.addEventListener("submit", (e) => __awaiter(this, void 0, void 0, function* () {
        e.preventDefault();
        const email = document.getElementById("email").value;
        const password = document.getElementById("password").value;
        const errorEl = document.getElementById("login-error");
        try {
            const res = yield fetch(`${API_BASE}/login`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                credentials: "include",
                body: JSON.stringify({ email, password }),
            });
            if (res.ok) {
                navigateTo("/");
            }
            else {
                errorEl.textContent = "Invalid email or password";
            }
        }
        catch (err) {
            console.error("Login error:", err);
            errorEl.textContent = "Network error";
        }
    }));
}
function renderRegisterPage() {
    var _a;
    const app = document.getElementById("app");
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
    (_a = document.getElementById("link-login")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", (e) => {
        e.preventDefault();
        navigateTo("/login");
    });
    const form = document.getElementById("register-form");
    form.addEventListener("submit", (e) => __awaiter(this, void 0, void 0, function* () {
        e.preventDefault();
        const displayName = document.getElementById("displayName").value;
        const email = document.getElementById("reg-email").value;
        const password = document.getElementById("reg-password").value;
        const errorEl = document.getElementById("register-error");
        try {
            const res = yield fetch(`${API_BASE}/register`, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ email, password, displayName }),
            });
            if (res.ok) {
                navigateTo("/login");
            }
            else {
                const errText = yield res.text();
                errorEl.textContent = errText || "Registration failed";
            }
        }
        catch (err) {
            console.error("Register error:", err);
            errorEl.textContent = "Network error";
        }
    }));
}
function renderHome() {
    var _a;
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>Home</h2>
    <p>Welcome! You are logged in.</p>
    <button id="logout-btn">Logout</button>
  `;
    (_a = document.getElementById("logout-btn")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", () => __awaiter(this, void 0, void 0, function* () {
        try {
            const res = yield fetch(`${API_BASE}/logout`, {
                method: "POST",
                credentials: "include",
            });
            if (res.ok) {
                navigateTo("/login");
            }
            else {
                alert("Logout failed");
            }
        }
        catch (_a) {
            alert("Network error");
        }
    }));
}
//# sourceMappingURL=main.js.map