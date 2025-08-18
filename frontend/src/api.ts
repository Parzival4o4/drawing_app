const API_BASE = "/api";

export async function isAuthenticated(): Promise<boolean> {
  try {
    const res = await fetch(`${API_BASE}/me`, { credentials: "include" });
    return res.ok;
  } catch {
    return false;
  }
}

export async function login(email: string, password: string) {
  return fetch(`${API_BASE}/login`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    credentials: "include",
    body: JSON.stringify({ email, password }),
  });
}

export async function register(email: string, password: string, display_name: string) {
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

export async function createCanvas(name: string) {
  return fetch(`${API_BASE}/canvases/create`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name }),
  });
}

export interface UserInfo {
  user_id: string;
  email: string;
  display_name: string;
}

export async function getUserInfo(): Promise<UserInfo> {
  const res = await fetch(`${API_BASE}/me`);
  if (!res.ok) throw new Error("Failed to fetch user info");
  return res.json();
}

export async function updateUserInfo(email?: string, display_name?: string) {
  return fetch(`${API_BASE}/user/update`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ email, display_name }),
  });
}
