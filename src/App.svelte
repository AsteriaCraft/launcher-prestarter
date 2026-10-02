<script lang="ts">
  import "reset-css";
  import "$lib/assets/css/global.scss";

  import { getVersion } from "@tauri-apps/api/app";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount, tick } from "svelte";

  import background from "$lib/assets/images/back.jpg";
  import DownloadBlock from "$lib/components/ui/DownloadBlock.svelte";
  import { commands, events, firstFrame } from "$lib/config/app";
  import { translator, type Translator } from "$lib/i18n";
  import type { Boot, FailurePayload, Notice, ProgressPayload, StagePayload } from "$lib/types/events";
  import { whenPaintable } from "$lib/utils/paint";
  import { initialState, reduce, type ViewEvent, type ViewState } from "$lib/utils/state";

  let view: ViewState = $state(initialState);
  let tr: Translator = $state(translator("en"));
  let version = $state("");
  let testMode = $state(false);
  let ready = $state(false);
  const unlisten: UnlistenFn[] = [];

  function dispatch(event: ViewEvent) {
    view = reduce(view, event);
  }

  const call = (command: string, args?: Record<string, unknown>) =>
    invoke(command, args).catch((err: unknown) => console.error(command, err));

  onMount(async () => {
    const boot = await invoke<Boot>(commands.boot);
    tr = translator(boot.lang);
    testMode = boot.testMode;
    document.documentElement.lang = boot.lang;
    version = await getVersion();

    unlisten.push(
      await listen<StagePayload>(events.stage, (e) => dispatch({ type: "stage", stage: e.payload.stage })),
      await listen<ProgressPayload>(events.progress, (e) =>
        dispatch({ type: "progress", done: e.payload.done, total: e.payload.total, at: performance.now() }),
      ),
      await listen<Notice>(events.notice, (e) => dispatch({ type: "notice", notice: e.payload })),
      await listen<FailurePayload>(events.failure, (e) => dispatch({ type: "failure", failure: e.payload })),
      await listen(events.done, () => dispatch({ type: "done" })),
    );
    ready = true;
    // Show the window once its first frame has what it paints (no white flash, no fallback font), then start the
    // work. Not requestAnimationFrame: WebKit runs none in a hidden window, so on Linux and macOS it never fired.
    await tick();
    await whenPaintable(firstFrame.fonts, [background], firstFrame.timeoutMs);
    call(commands.ready);
  });

  onDestroy(() => unlisten.forEach((stop) => stop()));
</script>

<svelte:head>
  <title>Asterium</title>
</svelte:head>

<div data-tauri-drag-region class="app">
  {#if ready}
    <DownloadBlock
      {view}
      {tr}
      {version}
      {testMode}
      onRetry={() => {
        dispatch({ type: "retry" });
        call(commands.retry);
      }}
      onOpenLogs={() => call(commands.openLogs)}
      onPlayNow={() => {
        dispatch({ type: "playNow" });
        call(commands.playNow);
      }}
      onOpenPage={(url) => call(commands.openPage, { url })}
      onClose={() => call(commands.quit)}
      onMinimize={() => call(commands.minimize)}
    />
  {/if}
</div>

<style lang="scss">
  :global(.app) {
    border-radius: 12px;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
    background: url("$lib/assets/images/back.jpg");
    background-size: cover;
    background-repeat: no-repeat;
    background-position: center;
    font-family: "Inter", "Kharkiv", sans-serif;
    color: $text-primary;
  }
</style>
