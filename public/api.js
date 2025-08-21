const API_BASE = "/api";
export async function isAuthenticated() {
    try {
        const res = await fetch(`${API_BASE}/me`, { credentials: "include" });
        return res.ok;
    }
    catch {
        return false;
    }
}
export async function login(email, password) {
    return fetch(`${API_BASE}/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        credentials: "include",
        body: JSON.stringify({ email, password }),
    });
}
export async function register(email, password, display_name) {
    return fetch(`${API_BASE}/register`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, password, display_name }),
    });
}
export async function logout() {
    return fetch(`${API_BASE}/logout`, {
        method: "POST",
        credentials: "include",
    });
}
// --- New API calls from home.ts ---
export async function getCanvases() {
    return fetch(`${API_BASE}/canvases/list`);
}
export async function createCanvas(name) {
    return fetch(`${API_BASE}/canvases/create`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ name }),
    });
}
export async function getUserInfo() {
    try {
        const res = await fetch(`${API_BASE}/me`, { credentials: "include" });
        if (!res.ok)
            return null;
        return await res.json();
    }
    catch {
        return null;
    }
}
export async function updateUserInfo(email, display_name) {
    return fetch(`${API_BASE}/user/update`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email, display_name }),
    });
}
//# sourceMappingURL=api.js.map