import { mount } from "svelte";
import "./app.css";
import Editor from "./components/Editor.svelte";

const app = mount(Editor, { target: document.getElementById("app")! });

export default app;