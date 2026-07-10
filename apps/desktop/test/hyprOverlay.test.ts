import { describe, expect, test } from "bun:test";
import { EventEmitter } from "node:events";
import type { Socket } from "node:net";
import {
  clearHyprOverlayImage,
  detectHyprOverlay,
  getHyprOverlayStatus,
  hideHyprOverlay,
  hyprlandSocket2Path,
  isInteractiveRegexMenuPayload,
  isValidHyprOverlayImageInput,
  parseHyprlandInstanceSignatures,
  parseHyprlandMonitorBounds,
  parseHyprOverlayEventLine,
  parseHyprOverlayStatus,
  REGEX_OVERLAY_ID,
  registerHyprOverlayImage,
  resolveHyprlandSocket2Path,
  sendHyprOverlay,
  sendHyprOverlaySelection,
  startHyprOverlayEventSession,
  startHyprOverlaySelectionListener,
  virtualDesktopBounds,
  waitForHyprOverlaySelection,
  type HyprctlRunner,
} from "../src/capture/hyprOverlay";

function runnerFor(map: Record<string, string>): HyprctlRunner {
  return async (args) => {
    const key = args.join("\0");
    const stdout = map[key];
    if (stdout === undefined) throw new Error(`unexpected hyprctl ${args.join(" ")}`);
    return { stdout, stderr: "" };
  };
}

class FakeSocket extends EventEmitter {
  destroyed = false;

  destroy(): this {
    this.destroyed = true;
    return this;
  }

  asSocket(): Socket {
    return this as unknown as Socket;
  }
}

describe("hypr-overlay hyprctl transport", () => {
  test("detects loaded plugin with healthy JSON status", async () => {
    const runner = runnerFor({
      ["plugin\0list"]: "Plugin hyproverlay by grok-insider\n",
      ["-j\0hyproverlay\0status"]: '{"loaded":true,"visible":false}',
    });
    expect(await detectHyprOverlay(runner)).toBe(true);
  });

  test("detect returns false when plugin list lacks hyproverlay", async () => {
    const runner = runnerFor({
      ["plugin\0list"]: "no plugins loaded\n",
    });
    expect(await detectHyprOverlay(runner)).toBe(false);
  });

  test("parses and queries detailed protocol v4 status", async () => {
    const raw = JSON.stringify({
      loaded: true,
      protocolVersion: 4,
      capabilities: ["cards.positionedRows", "images.rgba", "selection.dragConfirm"],
      limits: {
        payloadBytes: 16_384,
        rows: 48,
        imageDimension: 64,
        imagePayloadBytes: 24_576,
      },
      visible: true,
      mode: "selection",
      generation: 9,
      rows: 2,
      controls: 0,
      focusIndex: 0,
      inputFocused: false,
      hoverIndex: null,
      interactive: true,
      images: { count: 2, bytes: 128 },
      eventSeq: 7,
      ttlMs: 0,
      rect: { x: -1920, y: 0, w: 4480, h: 1440 },
    });
    const parsed = parseHyprOverlayStatus(raw);
    expect(parsed?.protocolVersion).toBe(4);
    expect(parsed?.capabilities).toContain("selection.dragConfirm");
    expect(parsed?.limits.imagePayloadBytes).toBe(24_576);
    expect(parsed?.images).toEqual({ count: 2, bytes: 128 });
    expect(parsed?.rect).toEqual({ x: -1920, y: 0, w: 4480, h: 1440 });

    const queried = await getHyprOverlayStatus(
      runnerFor({ ["-j\0hyproverlay\0status"]: raw }),
    );
    expect(queried?.mode).toBe("selection");
    expect(parseHyprOverlayStatus("not json")).toBeNull();
  });

  test("send serializes bounded payload as one hyprctl argument", async () => {
    const seen: string[][] = [];
    const runner: HyprctlRunner = async (args) => {
      seen.push(args);
      return { stdout: "ok\n", stderr: "" };
    };
    const ok = await sendHyprOverlay(
      {
        rect: { x: 1, y: 2, w: 3, h: 4 },
        rows: [{ label: "Divine Orb", value: "142 ex", emphasis: true }],
      },
      runner,
    );
    expect(ok).toBe(true);
    expect(seen[0]?.slice(0, 2)).toEqual(["hyproverlay", "set-json"]);
    expect(JSON.parse(seen[0]?.[2] ?? "{}").rows[0].label).toBe("Divine Orb");
  });

  test("send accepts generic menu payloads", async () => {
    const seen: string[][] = [];
    const runner: HyprctlRunner = async (args) => {
      seen.push(args);
      return { stdout: "ok\n", stderr: "" };
    };
    const ok = await sendHyprOverlay(
      {
        mode: "menu",
        rect: { x: 10, y: 20, w: 500, h: 400 },
        menu: {
          title: "Search Regex",
          activeTab: "items",
          tabs: [{ id: "items", label: "Items" }],
          controls: [{ id: "rare", tab: "items", label: "Rare", selected: true }],
        },
      },
      runner,
    );
    expect(ok).toBe(true);
    const payload = JSON.parse(seen[0]?.[2] ?? "{}");
    expect(payload.mode).toBe("menu");
    expect(payload.menu.controls[0].id).toBe("rare");
  });

  test("send accepts positioned icon rows and selection root config", async () => {
    const seen: string[][] = [];
    const runner: HyprctlRunner = async (args) => {
      seen.push(args);
      return { stdout: "ok\n", stderr: "" };
    };
    const ok = await sendHyprOverlaySelection(
      {
        visible: true,
        rect: { x: -1920, y: 0, w: 4480, h: 1440 },
        rows: [
          {
            kind: "header",
            label: "Region",
            top: 12,
            height: 32,
            iconId: "currency.divine",
            iconSize: 24,
            iconGap: 6,
          },
        ],
        interactive: {
          enabled: true,
          pointer: true,
          keyboard: true,
          overlayId: "region-picker",
        },
        selection: {
          draft: { x: 10, y: 20, w: 300, h: 200 },
          border: "#d29933ff",
          borderWidth: 2,
          hint: "Drag to select",
          hintColor: "#ffffffff",
          hintSize: 14,
        },
      },
      runner,
    );
    expect(ok).toBe(true);
    const payload = JSON.parse(seen[0]?.[2] ?? "{}");
    expect(payload.mode).toBe("selection");
    expect(payload.rows[0].top).toBe(12);
    expect(payload.rows[0].iconId).toBe("currency.divine");
    expect(payload.interactive.overlayId).toBe("region-picker");
    expect(payload.selection.draft).toEqual({ x: 10, y: 20, w: 300, h: 200 });
  });

  test("validates, registers, and clears bounded RGBA images", async () => {
    const image = {
      id: "currency.divine",
      width: 2,
      height: 2,
      rgbaBase64: Buffer.alloc(16, 0xff).toString("base64"),
    };
    expect(isValidHyprOverlayImageInput(image)).toBe(true);
    expect(isValidHyprOverlayImageInput({ ...image, id: "all" })).toBe(false);
    expect(isValidHyprOverlayImageInput({ ...image, rgbaBase64: "!!!!" })).toBe(false);
    expect(isValidHyprOverlayImageInput({ ...image, width: 65 })).toBe(false);

    const calls: string[][] = [];
    const runner: HyprctlRunner = async (args) => {
      calls.push(args);
      return { stdout: args[1] === "image-clear" ? "noop\n" : "ok\n", stderr: "" };
    };
    expect(await registerHyprOverlayImage(image, runner)).toBe(true);
    expect(calls[0]?.slice(0, 2)).toEqual(["hyproverlay", "image-set-json"]);
    expect(JSON.parse(calls[0]?.[2] ?? "{}")).toEqual(image);
    expect(await clearHyprOverlayImage("currency.divine", runner)).toBe(true);
    expect(await clearHyprOverlayImage("all", runner)).toBe(true);
    expect(await clearHyprOverlayImage("../bad", runner)).toBe(false);
    expect(calls).toHaveLength(3);
  });

  test("unions enabled monitors in compositor-global logical coordinates", async () => {
    const raw = JSON.stringify([
      { x: -1920, y: 0, width: 1920, height: 1080, scale: 1, disabled: false },
      { x: 0, y: -100, width: 3840, height: 2160, scale: 2, disabled: false },
      { x: 9000, y: 0, width: 100, height: 100, scale: 1, disabled: true },
    ]);
    expect(parseHyprlandMonitorBounds(raw)).toEqual({
      x: -1920,
      y: -100,
      w: 3840,
      h: 1180,
    });
    expect(parseHyprlandMonitorBounds("[]")).toBeNull();
    expect(parseHyprlandMonitorBounds("invalid")).toBeNull();

    const runner = runnerFor({ ["monitors\0-j"]: raw });
    expect(await virtualDesktopBounds(runner)).toEqual({
      x: -1920,
      y: -100,
      w: 3840,
      h: 1180,
    });
  });

  test("accounts for rotated monitor axes when deriving logical bounds", () => {
    const raw = JSON.stringify([
      { x: 0, y: 0, width: 1920, height: 1080, scale: 1, transform: 1 },
    ]);
    expect(parseHyprlandMonitorBounds(raw)).toEqual({ x: 0, y: 0, w: 1080, h: 1920 });
  });

  test("discovers socket2 paths from env or hyprctl instances", async () => {
    expect(
      parseHyprlandInstanceSignatures(
        JSON.stringify([{ instance: "first" }, { instance: "second" }, { instance: "../bad" }]),
      ),
    ).toEqual(["first", "second"]);
    expect(hyprlandSocket2Path("/run/user/1000", "active")).toBe(
      "/run/user/1000/hypr/active/.socket2.sock",
    );
    expect(hyprlandSocket2Path("/run/user/1000", "../bad")).toBeNull();

    let called = false;
    const envPath = await resolveHyprlandSocket2Path(
      {
        XDG_RUNTIME_DIR: "/run/user/1000",
        HYPRLAND_INSTANCE_SIGNATURE: "from-env",
      },
      async () => {
        called = true;
        throw new Error("runner should not be used");
      },
    );
    expect(envPath).toBe("/run/user/1000/hypr/from-env/.socket2.sock");
    expect(called).toBe(false);

    const discovered = await resolveHyprlandSocket2Path(
      { XDG_RUNTIME_DIR: "/run/user/1000" },
      runnerFor({ ["instances\0-j"]: JSON.stringify([{ instance: "from-list" }]) }),
    );
    expect(discovered).toBe("/run/user/1000/hypr/from-list/.socket2.sock");
  });

  test("parses only well-formed hyproverlay event lines", () => {
    const event = parseHyprOverlayEventLine(
      `hyproverlay>>${JSON.stringify({
        seq: 7,
        type: "submit",
        overlayId: "region-picker",
        rect: { x: -10, y: 20, w: 300, h: 200 },
      })}`,
    );
    expect(event?.rect).toEqual({ x: -10, y: 20, w: 300, h: 200 });
    expect(parseHyprOverlayEventLine("workspace>>1")).toBeNull();
    expect(parseHyprOverlayEventLine("hyproverlay>>not-json")).toBeNull();
    expect(
      parseHyprOverlayEventLine(
        'hyproverlay>>{"type":"submit","overlayId":"x","rect":{"x":0,"y":0,"w":0,"h":2}}',
      )?.rect,
    ).toBeUndefined();
  });

  test("starts socket2 listener before selection is sent and resolves matching submit", async () => {
    const socket = new FakeSocket();
    let openedPath = "";
    const started = startHyprOverlaySelectionListener("region-picker", {
      timeoutMs: 1000,
      env: {
        XDG_RUNTIME_DIR: "/run/user/1000",
        HYPRLAND_INSTANCE_SIGNATURE: "test-instance",
      },
      socketFactory: (path) => {
        openedPath = path;
        queueMicrotask(() => socket.emit("connect"));
        return socket.asSocket();
      },
    });
    const listener = await started;
    expect(openedPath).toBe("/run/user/1000/hypr/test-instance/.socket2.sock");

    const sent = await sendHyprOverlaySelection(
      {
        rect: { x: 0, y: 0, w: 1920, h: 1080 },
        interactive: { enabled: true, overlayId: "region-picker" },
      },
      async () => {
        socket.emit("data", Buffer.from("workspace>>2\nhyproverlay>>{bad}\n"));
        socket.emit(
          "data",
          Buffer.from(
            `hyproverlay>>${JSON.stringify({
              type: "submit",
              overlayId: "other",
              rect: { x: 1, y: 2, w: 3, h: 4 },
            })}\nhyproverlay>>${JSON.stringify({
              type: "submit",
              overlayId: "region-picker",
              rect: { x: 100, y: 120, w: 640, h: 480 },
            })}\n`,
          ),
        );
        return { stdout: "ok\n", stderr: "" };
      },
    );
    expect(sent).toBe(true);
    expect(await listener.promise).toEqual({ x: 100, y: 120, w: 640, h: 480 });
    expect(socket.destroyed).toBe(true);
  });

  test("wait resolves null on matching dismiss, timeout, or abort", async () => {
    const dismissSocket = new FakeSocket();
    const dismissed = waitForHyprOverlaySelection("picker", {
      timeoutMs: 1000,
      env: { XDG_RUNTIME_DIR: "/tmp", HYPRLAND_INSTANCE_SIGNATURE: "test" },
      socketFactory: () => {
        queueMicrotask(() => {
          dismissSocket.emit("connect");
          queueMicrotask(() =>
            dismissSocket.emit(
              "data",
              Buffer.from(
                'hyproverlay>>{"type":"dismiss","overlayId":"picker"}\n',
              ),
            ),
          );
        });
        return dismissSocket.asSocket();
      },
    });
    expect(await dismissed).toBeNull();

    const timeoutSocket = new FakeSocket();
    const timedOut = waitForHyprOverlaySelection("picker", {
      timeoutMs: 1,
      env: { XDG_RUNTIME_DIR: "/tmp", HYPRLAND_INSTANCE_SIGNATURE: "test" },
      socketFactory: () => {
        queueMicrotask(() => timeoutSocket.emit("connect"));
        return timeoutSocket.asSocket();
      },
    });
    expect(await timedOut).toBeNull();

    const abortSocket = new FakeSocket();
    const controller = new AbortController();
    const listenerPromise = startHyprOverlaySelectionListener("picker", {
      timeoutMs: 1000,
      signal: controller.signal,
      env: { XDG_RUNTIME_DIR: "/tmp", HYPRLAND_INSTANCE_SIGNATURE: "test" },
      socketFactory: () => {
        queueMicrotask(() => abortSocket.emit("connect"));
        return abortSocket.asSocket();
      },
    });
    const listener = await listenerPromise;
    controller.abort();
    expect(await listener.promise).toBeNull();
  });

  test("hide maps to hyproverlay hide", async () => {
    const runner = runnerFor({
      ["hyproverlay\0hide"]: "ok\n",
    });
    expect(await hideHyprOverlay(runner)).toBe(true);
  });

  test("parses selectedIdsTruncated on hyproverlay events", () => {
    const event = parseHyprOverlayEventLine(
      `hyproverlay>>${JSON.stringify({
        type: "change",
        overlayId: REGEX_OVERLAY_ID,
        controlId: "rare",
        selected: true,
        selectedIds: ["rare"],
        selectedIdsTruncated: true,
      })}`,
    );
    expect(event?.selectedIdsTruncated).toBe(true);
    expect(event?.selectedIds).toEqual(["rare"]);
  });

  test("isInteractiveRegexMenuPayload requires enabled overlayId", () => {
    expect(
      isInteractiveRegexMenuPayload({
        mode: "menu",
        visible: true,
        interactive: { enabled: true, overlayId: REGEX_OVERLAY_ID },
        rect: { x: 0, y: 0, w: 1, h: 1 },
      }),
    ).toBe(true);
    expect(
      isInteractiveRegexMenuPayload({
        mode: "menu",
        interactive: { enabled: true, overlayId: "other" },
        rect: { x: 0, y: 0, w: 1, h: 1 },
      }),
    ).toBe(false);
    expect(
      isInteractiveRegexMenuPayload({
        mode: "cards",
        interactive: { enabled: true, overlayId: REGEX_OVERLAY_ID },
        rect: { x: 0, y: 0, w: 1, h: 1 },
      }),
    ).toBe(false);
  });

  test("event session delivers matching events and ignores others until closed", async () => {
    const socket = new FakeSocket();
    const seen: string[] = [];
    const session = await startHyprOverlayEventSession(REGEX_OVERLAY_ID, {
      env: {
        XDG_RUNTIME_DIR: "/run/user/1000",
        HYPRLAND_INSTANCE_SIGNATURE: "test-instance",
      },
      socketFactory: () => {
        queueMicrotask(() => socket.emit("connect"));
        return socket.asSocket();
      },
      onEvent: (event) => {
        seen.push(`${event.type}:${event.controlId ?? ""}`);
      },
    });

    socket.emit(
      "data",
      Buffer.from(
        [
          `hyproverlay>>${JSON.stringify({
            type: "change",
            overlayId: "other",
            controlId: "x",
          })}`,
          `hyproverlay>>${JSON.stringify({
            type: "change",
            overlayId: REGEX_OVERLAY_ID,
            controlId: "rare",
            selectedIds: ["rare"],
          })}`,
          `hyproverlay>>${JSON.stringify({
            type: "focus",
            overlayId: REGEX_OVERLAY_ID,
            controlId: "magic",
          })}`,
          "",
        ].join("\n"),
      ),
    );

    await new Promise((r) => setTimeout(r, 10));
    expect(seen).toEqual(["change:rare", "focus:magic"]);

    session.close();
    expect(socket.destroyed).toBe(true);

    // Further data after close is ignored.
    socket.emit(
      "data",
      Buffer.from(
        `hyproverlay>>${JSON.stringify({
          type: "change",
          overlayId: REGEX_OVERLAY_ID,
          controlId: "normal",
        })}\n`,
      ),
    );
    await new Promise((r) => setTimeout(r, 10));
    expect(seen).toEqual(["change:rare", "focus:magic"]);
  });
});
