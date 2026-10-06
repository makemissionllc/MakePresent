import type { Look } from "./types";
import { DEFAULT_BODY_STYLE, DEFAULT_TITLE_STYLE } from "./types";

/** Layout shared by every renderer. Geometry uses CSS variables so fitting
 * can clear its temporary pixel width without erasing the Look's box. */
export function lookElementCss(look: Look, role: "title" | "body"): string {
  const style = (role === "title" ? look.titleStyle : look.bodyStyle)
    ?? (role === "title" ? DEFAULT_TITLE_STYLE : DEFAULT_BODY_STYLE);
  const box = role === "title" ? look.titleBox : look.bodyBox;
  const absolute = look.positioning === "absolute";
  const align = style.align ?? "center";
  const flexAlign = { left: "flex-start", center: "center", right: "flex-end" }[align];
  return [
    `text-align:${align}`,
    `--look-element-width:${absolute ? `${box.width}%` : "fit-content"}`,
    `--look-element-max-width:${absolute ? "none" : role === "body" ? "80%" : "100%"}`,
    `--look-element-height:${absolute ? `${box.height}%` : "auto"}`,
    `--look-element-align-self:${absolute ? "auto" : flexAlign}`,
    `--look-line-justify:${flexAlign}`,
    ...(absolute ? [`left:${box.x}%`, `top:${box.y}%`, `z-index:${box.zIndex}`] : []),
  ].join(";");
}
