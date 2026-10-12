import { mount } from "svelte";
import "./app.css";
import Editor from "./components/EditorRoot.svelte";
import { invoke } from "@tauri-apps/api/core";

function report(message: string): void {
  void invoke("report_frontend_status", { message }).catch(() => {});
}

window.addEventListener("error", (event) => {
  report(`error: ${event.message}\n${event.error?.stack ?? ""}`);
});
window.addEventListener("unhandledrejection", (event) => {
  report(`unhandled rejection: ${event.reason?.stack ?? String(event.reason)}`);
});
report("editor module loaded");

const app = mount(Editor, { target: document.getElementById("app")! });
report("editor mounted");

export default app;
