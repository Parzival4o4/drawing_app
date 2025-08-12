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
