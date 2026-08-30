import { mount } from "svelte";
import "./app.css";
import Output from "./components/Output.svelte";

const app = mount(Output, { target: document.getElementById("app")! });

export default app;