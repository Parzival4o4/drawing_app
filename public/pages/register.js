var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
import { register } from "../api.js";
import { navigateTo } from "../router.js";
export function renderRegisterPage() {
    var _a, _b;
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>Register</h2>
    <form id="register-form">
      <input type="email" id="reg-email" placeholder="Email" required />
      <br />
      <input type="text" id="displayName" placeholder="Display Name" required />
      <br />
      <input type="password" id="reg-password" placeholder="Password" required />
      <br />
      <button type="submit">Sign Up</button>
    </form>
    <p id="register-error" style="color: red;"></p>
    <p>Already have an account? <a href="/login" id="link-login">Login here</a></p>
  `;
    const emailInput = document.getElementById("reg-email");
    const displayNameInput = document.getElementById("displayName");
    // Suggest display name from email
    emailInput.addEventListener("input", () => {
        const emailValue = emailInput.value.trim();
        const suggestedName = emailValue.includes("@") ? emailValue.split("@")[0] : "";
        // Only set suggestion if display name is empty or matches previous suggestion
        if (!displayNameInput.value.trim() ||
            displayNameInput.dataset.autofilled === "true") {
            displayNameInput.value = suggestedName;
            displayNameInput.dataset.autofilled = "true";
        }
    });
    // If user edits the display name manually, stop autofill
    displayNameInput.addEventListener("input", () => {
        displayNameInput.dataset.autofilled = "false";
    });
    (_a = document.getElementById("link-login")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", (e) => {
        e.preventDefault();
        navigateTo("/login");
    });
    (_b = document.getElementById("register-form")) === null || _b === void 0 ? void 0 : _b.addEventListener("submit", (e) => __awaiter(this, void 0, void 0, function* () {
        e.preventDefault();
        const displayName = displayNameInput.value;
        const email = emailInput.value;
        const password = document.getElementById("reg-password").value;
        const errorEl = document.getElementById("register-error");
        try {
            const res = yield register(email, password, displayName);
            if (res.ok) {
                navigateTo("/");
            }
            else {
                errorEl.textContent = (yield res.text()) || "Registration failed";
            }
        }
        catch (err) {
            console.error("Register error:", err);
            errorEl.textContent = "Network error";
        }
    }));
}
//# sourceMappingURL=register.js.map