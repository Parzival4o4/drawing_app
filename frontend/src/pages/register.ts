import { register } from "../api.js";
import { navigateTo } from "../router.js";

export function renderRegisterPage() {
  const app = document.getElementById("app")!;
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

  const emailInput = document.getElementById("reg-email") as HTMLInputElement;
  const displayNameInput = document.getElementById("displayName") as HTMLInputElement;

  // Suggest display name from email
  emailInput.addEventListener("input", () => {
    const emailValue = emailInput.value.trim();
    const suggestedName = emailValue.includes("@") ? emailValue.split("@")[0] : "";

    // Only set suggestion if display name is empty or matches previous suggestion
    if (
      !displayNameInput.value.trim() ||
      displayNameInput.dataset.autofilled === "true"
    ) {
      displayNameInput.value = suggestedName;
      displayNameInput.dataset.autofilled = "true";
    }
  });

  // If user edits the display name manually, stop autofill
  displayNameInput.addEventListener("input", () => {
    displayNameInput.dataset.autofilled = "false";
  });

  document.getElementById("link-login")?.addEventListener("click", (e) => {
    e.preventDefault();
    navigateTo("/login");
  });

  document.getElementById("register-form")?.addEventListener("submit", async (e) => {
    e.preventDefault();
    const displayName = displayNameInput.value;
    const email = emailInput.value;
    const password = (document.getElementById("reg-password") as HTMLInputElement).value;
    const errorEl = document.getElementById("register-error")!;

    try {
      const res = await register(email, password, displayName);
      if (res.ok) {
        navigateTo("/");
      } else {
        errorEl.textContent = (await res.text()) || "Registration failed";
      }
    } catch (err) {
      console.error("Register error:", err);
      errorEl.textContent = "Network error";
    }
  });
}
