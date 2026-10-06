import { api } from "./sync";
import type { ClientState, MediaAsset } from "./types";

/** Use the same two-step slide path for cached media wherever it is browsed. */
export async function addCachedMediaSlide(asset: MediaAsset, insertItemAt?: number): Promise<ClientState> {
  const name = asset.fileName.replace(/\.[^/.]+$/, "") || "Media";
  const created = await api.addSlide("", "", name);
  const slideId = created.project.slides.at(-1)?.id;
  if (!slideId) throw new Error("Could not create a slide for this media file.");
  const updated = await api.updateSlide(slideId, { background: asset.background });
  if (insertItemAt === undefined) return updated;
  const itemId = updated.project.slides.find((slide) => slide.id === slideId)?.itemId;
  if (!itemId) throw new Error("Could not identify the new Playlist item.");
  return api.reorderItem(itemId, insertItemAt);
}
