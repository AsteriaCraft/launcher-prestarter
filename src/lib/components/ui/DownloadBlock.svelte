<script lang="ts">
  import { onDestroy } from "svelte";
  import { slide } from "svelte/transition";
  import ProgressBar from "./ProgressBar.svelte";
  import { ui } from "$lib/config/app";
  import type { Translator } from "$lib/i18n";
  import { canPlayNow, downloadStages, measuredStages, type ViewState } from "$lib/utils/state";
  import { formatBytes, formatSpeed, percent } from "$lib/utils/format";

  interface Props {
    view: ViewState;
    tr: Translator;
    version: string;
    testMode: boolean;
    onRetry: () => void;
    onOpenLogs: () => void;
    onPlayNow: () => void;
    onOpenPage: (url: string) => void;
    onClose: () => void;
    onMinimize: () => void;
  }

  let { view, tr, version, testMode, onRetry, onOpenLogs, onPlayNow, onOpenPage, onClose, onMinimize }: Props = $props();

  const tips = $derived(tr.tips());
  let tipIndex = $state(0);
  let copied = $state(false);
  const timer = setInterval(() => (tipIndex = tips.length ? (tipIndex + 1) % tips.length : 0), ui.tipIntervalMs);
  onDestroy(() => clearInterval(timer));

  const measured = $derived(measuredStages.has(view.stage) && view.total > 0);
  const pct = $derived(percent(view.done, view.total));
  const newer = $derived(view.notices.find((n) => n.kind === "newerWrapper"));

  async function copyCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      copied = false;
    }
  }
</script>

<div class="fullscreen-container">
  <div class="top-bar" data-tauri-drag-region>
    <span class="powered-by" data-tauri-drag-region>Powered by Sivium Solutions</span>
    <div class="window-buttons">
      <span class="build-version" data-tauri-drag-region>
        {#if testMode}<span class="badge">{tr.t("app.testMode")}</span>{/if}
        {tr.t("app.version", { version })}
      </span>
      <button type="button" aria-label="minimize" onclick={onMinimize}>&#8211;</button>
      <button type="button" aria-label={tr.t("action.close")} onclick={onClose}>&#215;</button>
    </div>
  </div>

  <div class="bottom-content">
    <div class="overlay-block">
      {#if view.failure}
        <div class="error-panel" transition:slide={{ duration: 200 }}>
          <p class="error-message">{view.failure.message}</p>
          {#if view.failure.command}
            <div class="command">
              <code>{view.failure.command}</code>
              <button type="button" onclick={() => copyCommand(view.failure?.command ?? "")}>
                {copied ? tr.t("action.copied") : tr.t("action.copy")}
              </button>
            </div>
          {/if}
          {#if view.failure.logTail}
            <pre class="log-tail">{view.failure.logTail}</pre>
          {/if}
          <div class="actions">
            {#if view.failure.retryable}
              <button type="button" class="primary" onclick={onRetry}>{tr.t("action.retry")}</button>
            {/if}
            {#if view.failure.link}
              <button type="button" class="primary" onclick={() => onOpenPage(view.failure?.link ?? "")}>{tr.t("action.download")}</button>
            {/if}
            <button type="button" onclick={onOpenLogs}>{tr.t("action.openLogs")}</button>
          </div>
        </div>
      {:else}
        {#each view.notices as notice (notice.kind)}
          {#if notice.kind === "jreUpdate" && canPlayNow(view)}
            <div class="notice" transition:slide={{ duration: 200 }}>
              <span>{tr.t("notice.jreUpdate")}</span>
              <button type="button" class="primary" onclick={onPlayNow}>{tr.t("action.playNow")}</button>
            </div>
          {:else if notice.kind === "translocated"}
            <div class="notice" transition:slide={{ duration: 200 }}><span>{tr.t("notice.translocated")}</span></div>
          {/if}
        {/each}
        {#if newer && newer.kind === "newerWrapper"}
          <div class="notice" transition:slide={{ duration: 200 }}>
            <span>{tr.t("notice.newerWrapper", { version: newer.version })}</span>
            <button type="button" onclick={() => onOpenPage(newer.page)}>{tr.t("action.download")}</button>
          </div>
        {/if}
      {/if}

      <div class="info-row">
        <span class="status-text">{tr.t(`stage.${view.stage}`)}</span>
        <div class="right-info">
          {#if measured && !view.failure}
            {#if downloadStages.has(view.stage) && view.speed !== null}
              <span class="speed">{formatSpeed(view.speed, tr)}</span>
            {/if}
            {#if downloadStages.has(view.stage)}
              <span class="bytes">{tr.t("progress.of", { done: formatBytes(view.done, tr), total: formatBytes(view.total, tr) })}</span>
            {/if}
            <span class="percentage">{Math.round(pct)}%</span>
          {/if}
        </div>
      </div>
      <ProgressBar percentage={pct} indeterminate={!measured && !view.finished} errored={view.failure !== null} />
    </div>

    <div class="welcome-panel">
      <h2>{tr.t("app.welcome")}</h2>
      {#if tips.length && !view.failure}
        {#key tipIndex}
          <p transition:slide={{ duration: 300 }}>{tips[tipIndex]}</p>
        {/key}
      {/if}
    </div>
  </div>
</div>

<style lang="scss">
  .fullscreen-container {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
  }

  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.75rem 0.75rem 0 1.25rem;
    font-size: 0.6rem;
    color: rgba(255, 255, 255, 0.5);

    .badge {
      margin-right: 0.5rem;
      padding: 0.1rem 0.4rem;
      border-radius: 0.3rem;
      background: rgba(255, 196, 0, 0.2);
      color: #ffd25e;
      font-weight: 600;
    }
  }

  .window-buttons {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    .build-version {
      margin-right: 0.5rem;
    }
  }

  .window-buttons button {
    width: 1.6rem;
    height: 1.4rem;
    border: none;
    border-radius: 0.3rem;
    background: transparent;
    color: rgba(255, 255, 255, 0.7);
    font-size: 0.9rem;
    cursor: pointer;
    &:hover {
      background: rgba(255, 255, 255, 0.12);
      color: $text-primary;
    }
  }

  .overlay-block {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .notice,
  .error-panel {
    margin: 0 1rem;
    padding: 0.6rem 0.8rem;
    border-radius: 0.5rem;
    font-size: 0.7rem;
    line-height: 1.35;
  }

  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    background: rgba(15, 20, 36, 0.85);
    border: 1px solid rgba(255, 255, 255, 0.08);
    color: rgba(255, 255, 255, 0.85);
  }

  .error-panel {
    background: rgba(20, 8, 12, 0.92);
    border: 1px solid rgba(255, 107, 107, 0.4);
    color: $text-primary;
    max-height: 9.5rem;
    overflow-y: auto;
    user-select: text;

    .error-message {
      white-space: pre-line;
      font-weight: 600;
    }

    .command {
      display: flex;
      gap: 0.5rem;
      align-items: center;
      margin-top: 0.4rem;
      code {
        flex: 1;
        padding: 0.3rem 0.5rem;
        border-radius: 0.3rem;
        background: rgba(255, 255, 255, 0.08);
        font-size: 0.65rem;
        user-select: all;
      }
    }

    .log-tail {
      margin-top: 0.4rem;
      max-height: 3.6rem;
      overflow: auto;
      font-size: 0.55rem;
      color: rgba(255, 255, 255, 0.6);
      white-space: pre-wrap;
    }

    .actions {
      display: flex;
      gap: 0.5rem;
      margin-top: 0.5rem;
    }
  }

  button {
    font: inherit;
    font-size: 0.65rem;
    padding: 0.3rem 0.7rem;
    border-radius: 0.4rem;
    border: 1px solid rgba(255, 255, 255, 0.15);
    background: rgba(255, 255, 255, 0.06);
    color: $text-primary;
    cursor: pointer;
    white-space: nowrap;
    &:hover {
      background: rgba(255, 255, 255, 0.14);
    }
    &.primary {
      border: none;
      background: $active;
      font-weight: 600;
    }
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.75rem;
    padding: 0 1.25rem;

    .status-text {
      color: rgba(255, 255, 255, 0.75);
    }

    .right-info {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      .speed,
      .bytes {
        color: rgba(255, 255, 255, 0.6);
        font-size: 0.65rem;
      }
      .percentage {
        color: $text-primary;
        font-weight: 700;
        font-size: 0.85rem;
      }
    }
  }

  .welcome-panel {
    background: rgba(0, 0, 0, 0.9);
    padding: 0.9rem 1.25rem;
    border-radius: 0 0 12px 12px;
    min-height: 3.6rem;

    h2 {
      color: $text-primary;
      font-size: 0.9rem;
      font-weight: 600;
      margin: 0 0 0.25rem 0;
    }

    p {
      color: rgba(255, 255, 255, 0.6);
      font-size: 0.7rem;
      line-height: 1.3;
    }
  }
</style>
