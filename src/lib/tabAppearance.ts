import {
  AlertTriangle,
  Archive,
  Bell,
  BookOpen,
  BrainCircuit,
  Briefcase,
  Building2,
  Calendar,
  Camera,
  Clock,
  Code2,
  Coffee,
  CreditCard,
  Dumbbell,
  FileText,
  Folder,
  Gamepad2,
  Gift,
  Github,
  Globe,
  GraduationCap,
  Handshake,
  Heart,
  Home,
  Inbox,
  Lightbulb,
  Mail,
  MailOpen,
  MapPin,
  Megaphone,
  MessageCircle,
  MessagesSquare,
  Music,
  Newspaper,
  Palette,
  Plane,
  Puzzle,
  Receipt,
  Rss,
  Send,
  ShieldCheck,
  ShoppingBag,
  Star,
  Stethoscope,
  Tag,
  Ticket,
  Trash2,
  Users,
  Utensils,
  Wallet
} from "lucide-svelte";
import type { Tab } from "./types";
import { shiftColor, type ColorVisionMode } from "./theme/colorVision";

export const TAB_COLORS = [
  "#6da4ff",
  "#58c98b",
  "#f2b85b",
  "#e879b9",
  "#b489ff",
  "#f28f6b",
  "#6dd4d4",
  "#c4d36a"
];

export type TabIconOption = {
  id: string;
  label: string;
  component: typeof Inbox;
};

export const tabIcons: TabIconOption[] = [
  { id: "inbox", label: "Inbox", component: Inbox },
  { id: "mail", label: "Mail", component: Mail },
  { id: "mail-open", label: "Open mail", component: MailOpen },
  { id: "send", label: "Sent", component: Send },
  { id: "archive", label: "Archive", component: Archive },
  { id: "trash", label: "Trash", component: Trash2 },
  { id: "star", label: "Star", component: Star },
  { id: "bell", label: "Updates", component: Bell },
  { id: "tag", label: "Tag", component: Tag },
  { id: "users", label: "People", component: Users },
  { id: "folder", label: "Folder", component: Folder },
  { id: "briefcase", label: "Work", component: Briefcase },
  { id: "building", label: "Company", component: Building2 },
  { id: "handshake", label: "Partners", component: Handshake },
  { id: "sparkles", label: "Aura", component: BrainCircuit },
  { id: "message", label: "Conversation", component: MessageCircle },
  { id: "messages", label: "Threads", component: MessagesSquare },
  { id: "file", label: "Documents", component: FileText },
  { id: "receipt", label: "Receipts", component: Receipt },
  { id: "wallet", label: "Finance", component: Wallet },
  { id: "card", label: "Payments", component: CreditCard },
  { id: "shopping", label: "Shopping", component: ShoppingBag },
  { id: "gift", label: "Gifts", component: Gift },
  { id: "ticket", label: "Tickets", component: Ticket },
  { id: "megaphone", label: "Promotions", component: Megaphone },
  { id: "newspaper", label: "News", component: Newspaper },
  { id: "rss", label: "Feeds", component: Rss },
  { id: "calendar", label: "Calendar", component: Calendar },
  { id: "clock", label: "Reminders", component: Clock },
  { id: "plane", label: "Travel", component: Plane },
  { id: "map", label: "Places", component: MapPin },
  { id: "home", label: "Home", component: Home },
  { id: "heart", label: "Favourites", component: Heart },
  { id: "book", label: "Reading", component: BookOpen },
  { id: "education", label: "Education", component: GraduationCap },
  { id: "music", label: "Music", component: Music },
  { id: "camera", label: "Photos", component: Camera },
  { id: "games", label: "Games", component: Gamepad2 },
  { id: "food", label: "Food", component: Utensils },
  { id: "coffee", label: "Coffee", component: Coffee },
  { id: "fitness", label: "Fitness", component: Dumbbell },
  { id: "health", label: "Health", component: Stethoscope },
  { id: "code", label: "Development", component: Code2 },
  { id: "github", label: "GitHub", component: Github },
  { id: "globe", label: "Web", component: Globe },
  { id: "design", label: "Design", component: Palette },
  { id: "ideas", label: "Ideas", component: Lightbulb },
  { id: "security", label: "Security", component: ShieldCheck },
  { id: "alerts", label: "Alerts", component: AlertTriangle },
  { id: "puzzle", label: "Other", component: Puzzle }
];

const iconMap = new Map(tabIcons.map((icon) => [icon.id, icon]));

export function isHexColor(value: string): boolean {
  return /^#[0-9a-f]{6}$/i.test(value.trim());
}

// Even Oklab-lightness steps, so the eight built-in colours stay distinguishable
// in grey. Dark themes use lighter greys; light themes use darker greys.
const ACHROMATOPSIA_DARK_LADDER = [
  "#dedede",
  "#c9c9c9",
  "#b4b4b4",
  "#a0a0a0",
  "#8c8c8c",
  "#797979",
  "#666666",
  "#545454"
];
const ACHROMATOPSIA_LIGHT_LADDER = [
  "#060606",
  "#121212",
  "#1f1f1f",
  "#2e2e2e",
  "#3d3d3d",
  "#4d4d4d",
  "#5d5d5d",
  "#6f6f6f"
];

export function tabColor(
  tab: Tab,
  index: number,
  mode: ColorVisionMode = "none",
  dark = true
): string {
  const color = tab.color?.trim();
  const stored = color && isHexColor(color) ? color : null;
  const builtinIndex = stored
    ? TAB_COLORS.findIndex(
        (candidate) => candidate.toLowerCase() === stored.toLowerCase()
      )
    : -1;
  const ladder = dark
    ? ACHROMATOPSIA_DARK_LADDER
    : ACHROMATOPSIA_LIGHT_LADDER;

  if (mode === "achromatopsia") {
    if (builtinIndex >= 0) return ladder[builtinIndex];
    if (!stored) {
      return ladder[index % ladder.length];
    }
  }

  return shiftColor(stored ?? TAB_COLORS[index % TAB_COLORS.length], mode);
}

export function tabIconId(tab: Tab): string {
  const id = tab.icon?.trim();
  return id && iconMap.has(id) ? id : "folder";
}

export function tabIconComponent(tab: Tab): typeof Inbox {
  return iconMap.get(tabIconId(tab))?.component ?? Folder;
}

export function tabIconSearch(query: string): TabIconOption[] {
  const term = query.trim().toLowerCase();
  if (!term) return tabIcons;
  return tabIcons.filter(
    (icon) =>
      icon.id.includes(term) || icon.label.toLowerCase().includes(term)
  );
}
