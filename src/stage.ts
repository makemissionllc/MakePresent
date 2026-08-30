import { mount } from "svelte";
import "./app.css";
import Stage from "./components/Stage.svelte";

const app = mount(Stage, { target: document.getElementById("app")! });

export default app;