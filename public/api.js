var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
const API_BASE = "/api";
export function isAuthenticated() {
    return __awaiter(this, void 0, void 0, function* () {
        try {
            const res = yield fetch(`${API_BASE}/me`, { credentials: "include" });
            return res.ok;
        }
        catch (_a) {
            return false;
        }
    });
}
export function login(email, password) {
    return __awaiter(this, void 0, void 0, function* () {
        return fetch(`${API_BASE}/login`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            credentials: "include",
            body: JSON.stringify({ email, password }),
        });
    });
}
export function register(email, password, display_name) {
    return __awaiter(this, void 0, void 0, function* () {
        return fetch(`${API_BASE}/register`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ email, password, display_name }),
        });
    });
}
export function logout() {
    return __awaiter(this, void 0, void 0, function* () {
        return fetch(`${API_BASE}/logout`, {
            method: "POST",
            credentials: "include",
        });
    });
}
//# sourceMappingURL=api.js.map