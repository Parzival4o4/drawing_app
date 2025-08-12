var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
import { logout } from "../api.js";
import { navigateTo } from "../router.js";
export function renderHome() {
    var _a;
    const app = document.getElementById("app");
    app.innerHTML = `
    <h2>Home</h2>
    <p>Welcome! You are logged in.</p>
    <button id="logout-btn">Logout</button>
  `;
    (_a = document.getElementById("logout-btn")) === null || _a === void 0 ? void 0 : _a.addEventListener("click", () => __awaiter(this, void 0, void 0, function* () {
        try {
            const res = yield logout();
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
//# sourceMappingURL=home.js.map