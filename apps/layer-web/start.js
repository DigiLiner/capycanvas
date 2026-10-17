import { browserSupport, showGpuNotice } from "./gpu.js";

if (browserSupport().ui) import("./app.js");
else {
  document.body.dataset.theme = matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  document.body.dataset.gpu = "unavailable";
  showGpuNotice({ container: document.getElementById("gpu-notice") });
}
