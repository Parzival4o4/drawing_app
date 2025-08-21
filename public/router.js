import { renderLoginPage } from "./pages/login.js";
import { renderRegisterPage } from "./pages/register.js";
import { renderHome } from "./pages/home.js";
import { renderCanvasPage } from "./pages/canvas.js";
import { getUserInfo, isAuthenticated } from "./api.js";
export async function handleRoute() {
    const path = window.location.pathname;
    if (path === "/login") {
        renderLoginPage();
    }
    else if (path === "/register") {
        renderRegisterPage();
    }
    else if (path.startsWith("/canvas/")) {
        const id = path.split("/")[2]; // extract canvas id
        const userInfo = await getUserInfo();
        if (userInfo) {
            renderCanvasPage(id, userInfo.user_id);
        }
        else {
            navigateTo("/login");
        }
    }
    else if (path === "/") {
        if (await isAuthenticated()) {
            renderHome();
        }
        else {
            navigateTo("/login");
        }
    }
    else {
        navigateTo("/");
    }
}
export function navigateTo(path) {
    history.pushState(null, "", path);
    handleRoute();
}
//# sourceMappingURL=router.js.map