import "@testing-library/jest-dom/vitest";

if (!Element.prototype.animate) {
  Object.defineProperty(Element.prototype, "animate", {
    configurable: true,
    value: () => {
      const animation = {
        currentTime: 0,
        effect: null,
        onfinish: null as (() => void) | null,
        playState: "finished",
        cancel() {
          animation.playState = "idle";
          animation.onfinish = null;
        }
      };
      queueMicrotask(() => {
        if (animation.playState === "finished") animation.onfinish?.();
      });
      return animation;
    }
  });
}
