import { handleRoute } from "./router.js";

document.addEventListener("DOMContentLoaded", () => {
  handleRoute();
  window.addEventListener("popstate", () => handleRoute());
});
