import type { ServicePreset } from "./types";

export const PRESETS: ServicePreset[] = [
  {
    id: "sunday-morning",
    name: "Sunday Morning Service",
    category: "Sunday Service",
    description: "Welcome, worship, scripture, sermon & closing — classic Sunday flow.",
    defaultAspect: "16:9",
    playlistItems: [
      { title: "Welcome", type: "slide", content: "Welcome to Worship\nWe're glad you're here!" },
      { title: "Worship — Amazing Grace", type: "song", referenceId: "amazing-grace", content: "Amazing grace, how sweet the sound\nThat saved a wretch like me" },
      { title: "Worship — Faithfulness", type: "song", referenceId: "great-is-thy-faithfulness", content: "Great is Thy faithfulness, O God my Father" },
      { title: "Scripture Reading", type: "scripture", referenceId: "John 3:16", content: "For God so loved the world… — John 3:16" },
      { title: "Sermon Outline — Title", type: "slide", content: "Today's Message\nSpeaker: Pastor\nText: John 3:16" },
      { title: "Closing Announcement", type: "slide", content: "Thanks for joining!\nSee you next Sunday" },
    ],
  },
  {
    id: "midweek",
    name: "Midweek Prayer & Bible Study",
    category: "Midweek",
    description: "Opening prayer, verse-by-verse study & prayer requests.",
    defaultAspect: "16:9",
    playlistItems: [
      { title: "Opening Prayer", type: "slide", content: "Opening Prayer\nLet us pray together" },
      { title: "Scripture — Psalm 23:1", type: "scripture", referenceId: "Psalm 23:1", content: "The Lord is my shepherd; I shall not want." },
      { title: "Scripture — Psalm 23:2", type: "scripture", referenceId: "Psalm 23:2", content: "He makes me lie down in green pastures." },
      { title: "Scripture — Psalm 23:4", type: "scripture", referenceId: "Psalm 23:4", content: "Even though I walk through the darkest valley…" },
      { title: "Prayer Requests", type: "slide", content: "Prayer Requests\nShare your burdens" },
      { title: "Closing Blessing", type: "slide", content: "Go in peace — see you Sunday" },
    ],
  },
  {
    id: "youth",
    name: "Youth Event — Upbeat Service",
    category: "Youth",
    description: "High-energy songs, games & announcements for youth night.",
    defaultAspect: "16:9",
    playlistItems: [
      { title: "Welcome — Youth Night!", type: "slide", content: "YOUTH NIGHT\nAre you ready?" },
      { title: "Upbeat Worship", type: "song", referenceId: "youth-worship", content: "This is the day the Lord has made\nWe will rejoice!" },
      { title: "Game — Ice Breaker", type: "slide", content: "Quick Game\nTwo Truths & a Lie" },
      { title: "Announcements", type: "slide", content: "Upcoming Events\nRetreat — Dec 12" },
      { title: "Message — Live Boldly", type: "slide", content: "Live boldly for Christ\n1 Timothy 4:12" },
    ],
  },
  {
    id: "blank",
    name: "Blank / Custom Service",
    category: "Custom",
    description: "Empty canvas — start from scratch.",
    defaultAspect: "16:9",
    playlistItems: [],
  },
];

export function presetGradient(id: string): string {
  if (id === "sunday-morning") return "linear-gradient(135deg,#0f2b4a 0%,#123a5c 50%,#1f3a2f 100%)";
  if (id === "midweek") return "linear-gradient(135deg,#1f3a2f 0%,#0f2b4a 100%)";
  if (id === "youth") return "linear-gradient(135deg,#ff7a18 0%,#ff3b30 60%,#9b59b6 100%)";
  return "linear-gradient(135deg,#1a1a24 0%,#2b2b3d 100%)";
}
