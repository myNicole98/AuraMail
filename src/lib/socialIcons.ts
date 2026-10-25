import discord from "../../resources/icons/social/Discord.svg?raw";
import facebook from "../../resources/icons/social/Facebook.svg?raw";
import github from "../../resources/icons/social/Github.svg?raw";
import instagram from "../../resources/icons/social/Instagram.svg?raw";
import linkedin from "../../resources/icons/social/LinkedIN.svg?raw";
import patreon from "../../resources/icons/social/Patreon.svg?raw";
import paypal from "../../resources/icons/social/PayPal.svg?raw";
import pinterest from "../../resources/icons/social/Pinterest.svg?raw";
import reddit from "../../resources/icons/social/Reddit.svg?raw";
import snapchat from "../../resources/icons/social/Snapchat.svg?raw";
import spotify from "../../resources/icons/social/Spotify.svg?raw";
import telegram from "../../resources/icons/social/Telegram.svg?raw";
import tiktok from "../../resources/icons/social/Tik Tok.svg?raw";
import tumblr from "../../resources/icons/social/Tumblr.svg?raw";
import twitch from "../../resources/icons/social/Twitch.svg?raw";
import whatsapp from "../../resources/icons/social/WhatsApp.svg?raw";
import youtube from "../../resources/icons/social/Youtube.svg?raw";

export type SocialNetwork = {
  id: string;
  label: string;
  aliases: string[];
  hosts: string[];
  urlPrefix: string;
  src: string;
};

const GENERIC_SVG =
  '<svg width="48" height="48" viewBox="0 0 48 48" xmlns="http://www.w3.org/2000/svg"><circle cx="24" cy="24" r="24" fill="#6b7280"/><path d="M19 29a6 6 0 0 1 0-8l3-3a6 6 0 0 1 8 0m-1 1a6 6 0 0 1 0 8l-3 3a6 6 0 0 1-8 0" fill="none" stroke="#fff" stroke-width="2.4" stroke-linecap="round"/></svg>';

function dataUri(svg: string): string {
  return `data:image/svg+xml;base64,${btoa(svg)}`;
}

function network(
  id: string,
  label: string,
  svg: string,
  aliases: string[],
  hosts: string[],
  urlPrefix: string
): SocialNetwork {
  return { id, label, aliases, hosts, urlPrefix, src: dataUri(svg) };
}

export const SOCIAL_NETWORKS: SocialNetwork[] = [
  network("linkedin", "LinkedIn", linkedin, ["linkedin", "linked in"], ["linkedin.com", "lnkd.in"], "https://www.linkedin.com/in/"),
  network("github", "GitHub", github, ["github", "gh"], ["github.com"], "https://github.com/"),
  network("facebook", "Facebook", facebook, ["facebook", "fb"], ["facebook.com", "fb.com"], "https://www.facebook.com/"),
  network("instagram", "Instagram", instagram, ["instagram", "ig"], ["instagram.com"], "https://www.instagram.com/"),
  network("discord", "Discord", discord, ["discord"], ["discord.com", "discord.gg"], "https://discord.com/users/"),
  network("patreon", "Patreon", patreon, ["patreon"], ["patreon.com"], "https://www.patreon.com/"),
  network("paypal", "PayPal", paypal, ["paypal"], ["paypal.com", "paypal.me"], "https://paypal.me/"),
  network("pinterest", "Pinterest", pinterest, ["pinterest"], ["pinterest.com"], "https://www.pinterest.com/"),
  network("reddit", "Reddit", reddit, ["reddit"], ["reddit.com"], "https://www.reddit.com/user/"),
  network("snapchat", "Snapchat", snapchat, ["snapchat", "snap"], ["snapchat.com"], "https://www.snapchat.com/add/"),
  network("spotify", "Spotify", spotify, ["spotify"], ["spotify.com"], "https://open.spotify.com/user/"),
  network("telegram", "Telegram", telegram, ["telegram", "tg"], ["t.me", "telegram.me", "telegram.org"], "https://t.me/"),
  network("tiktok", "TikTok", tiktok, ["tiktok", "tik tok", "tik-tok"], ["tiktok.com"], "https://www.tiktok.com/@"),
  network("tumblr", "Tumblr", tumblr, ["tumblr"], ["tumblr.com"], "https://www.tumblr.com/"),
  network("twitch", "Twitch", twitch, ["twitch"], ["twitch.tv"], "https://www.twitch.tv/"),
  network("whatsapp", "WhatsApp", whatsapp, ["whatsapp", "whats app"], ["wa.me", "whatsapp.com"], "https://wa.me/"),
  network("youtube", "YouTube", youtube, ["youtube", "yt"], ["youtube.com", "youtu.be"], "https://www.youtube.com/@")
];

export const GENERIC_SOCIAL_SRC = dataUri(GENERIC_SVG);

function normalize(value: string): string {
  return value.trim().toLowerCase();
}

function hostOf(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "";
  try {
    return new URL(trimmed).hostname.toLowerCase().replace(/^www\./, "");
  } catch {
    return "";
  }
}

export function socialNetworkFor(label: string, url: string): SocialNetwork | null {
  const text = normalize(label);
  if (text) {
    const byAlias = SOCIAL_NETWORKS.find(
      (candidate) =>
        candidate.id === text ||
        candidate.label.toLowerCase() === text ||
        candidate.aliases.includes(text)
    );
    if (byAlias) return byAlias;
  }
  const host = hostOf(url);
  if (host) {
    return (
      SOCIAL_NETWORKS.find((candidate) =>
        candidate.hosts.some(
          (domain) => host === domain || host.endsWith(`.${domain}`)
        )
      ) ?? null
    );
  }
  return null;
}

export function socialIconSrc(label: string, url: string): string {
  return socialNetworkFor(label, url)?.src ?? GENERIC_SOCIAL_SRC;
}
