var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
import { login } from "../api.js";
import { navigateTo } from "../router.js";
export function renderLoginPage() {
    var _a, _b;
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
    <p id="login-error"></p>
    <p>Don't have an account? <a href="/register" id="link-register">Register here</a></p>
  `;
    (_a = document.getElementById("link-register")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", (e) => {
        e.preventDefault();
        navigateTo("/register");
    });
    (_b = document.getElementById("login-form")) === null || _b === void 0 ? void 0 : _b.addEventListener("submit", (e) => __awaiter(this, void 0, void 0, function* () {
        e.preventDefault();
        const email = document.getElementById("email").value;
        const password = document.getElementById("password").value;
        const errorEl = document.getElementById("login-error");
        try {
            const res = yield login(email, password);
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
//# sourceMappingURL=login.js.map