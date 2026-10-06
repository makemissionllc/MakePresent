import { api } from "./sync";
import type { ClientState, MediaAsset } from "./types";

/** Use the same two-step slide path for cached media wherever it is browsed. */
export async function addCachedMediaSlide(asset: MediaAsset, insertAt?: number): Promise<ClientState> {
  const name = asset.fileName.replace(/\.[^/.]+$/, "") || "Media";
  const created = await api.addSlide("", "", name);
  const slideId = created.project.slides.at(-1)?.id;
  if (!slideId) throw new Error("Could not create a slide for this media file.");
  const updated = await api.updateSlide(slideId, { background: asset.background });
  if (insertAt === undefined || insertAt >= updated.project.slides.length - 1) return updated;
  const ids = updated.project.slides.map((slide) => slide.id);
  const from = ids.indexOf(slideId);
  if (from < 0) throw new Error("Could not place this media slide in the Playlist.");
  ids.splice(from, 1);
  ids.splice(Math.min(ids.length, Math.max(0, insertAt)), 0, slideId);
  return api.reorderSlides(ids);
}
