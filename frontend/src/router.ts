import { renderLoginPage } from "./pages/login.js";
import { renderRegisterPage } from "./pages/register.js";
import { renderHome } from "./pages/home.js";
import { isAuthenticated } from "./api.js";

export async function handleRoute() {
  const path = window.location.pathname;

  if (path === "/login") {
    renderLoginPage();
  } else if (path === "/register") {
    renderRegisterPage();
  } else if (path === "/") {
    if (await isAuthenticated()) {
      renderHome();
    } else {
      navigateTo("/login");
    }
  } else {
    navigateTo("/");
  }
}

export function navigateTo(path: string) {
  history.pushState(null, "", path);
  handleRoute();
}
