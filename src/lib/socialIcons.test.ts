import { describe, expect, it } from "vitest";
import {
  GENERIC_SOCIAL_SRC,
  SOCIAL_NETWORKS,
  socialIconSrc,
  socialNetworkFor
} from "./socialIcons";

describe("social icons", () => {
  it("has an embedded image for every supported network", () => {
    expect(SOCIAL_NETWORKS.length).toBe(17);
    for (const network of SOCIAL_NETWORKS) {
      expect(network.src).toMatch(/^data:image\/svg\+xml;base64,/);
      expect(network.urlPrefix).toMatch(/^https:\/\//);
    }
  });

  it("matches labels, aliases, and profile URLs", () => {
    expect(socialNetworkFor("LinkedIn", "")?.id).toBe("linkedin");
    expect(socialNetworkFor("YT", "")?.id).toBe("youtube");
    expect(socialNetworkFor("Tik Tok", "")?.id).toBe("tiktok");
    expect(socialNetworkFor("", "https://github.com/nicole")?.id).toBe("github");
    expect(socialNetworkFor("", "https://youtu.be/abc")?.id).toBe("youtube");
  });

  it("falls back to a generic icon", () => {
    expect(socialNetworkFor("Portfolio", "https://example.com")).toBeNull();
    expect(socialIconSrc("Portfolio", "https://example.com")).toBe(GENERIC_SOCIAL_SRC);
  });
});
