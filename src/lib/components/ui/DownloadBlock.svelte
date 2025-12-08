<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { slide } from "svelte/transition";
  import ProgressBar from "./ProgressBar.svelte";
  import { appConfig } from "$lib/config/app";
  import { tips } from "$lib/config/tips";

  export let error;
  export let speedMb;
  export let percentage: number;
  export let loadingStage: string = "Loading modules...";

  let currentTipIndex = Math.floor(Math.random() * tips.length);
  let tipInterval: ReturnType<typeof setInterval> | undefined;

  $: percentageLabel = Number.isFinite(percentage)
    ? `${Math.round(percentage)}%`
    : "0%";

  onMount(() => {
    tipInterval = setInterval(() => {
      currentTipIndex = Math.floor(Math.random() * tips.length);
    }, 5000);
  });

  onDestroy(() => {
    if (tipInterval) clearInterval(tipInterval);
  });

  // Stop tips rotation when error occurs
  $: if (error && tipInterval) {
    clearInterval(tipInterval);
    tipInterval = undefined;
  }
</script>

<div class="fullscreen-container">
  <!-- Top metadata -->
  <div class="top-bar">
    <span class="powered-by">Powered by Sivium Solutions</span>
    <span class="build-version">build v{appConfig.version} Alpha</span>
  </div>

  <!-- Bottom content area -->
  <div class="bottom-content">
    <!-- Semi-transparent overlay block -->
    <div class="overlay-block">
      <!-- Error message above everything -->
      {#if error}
        <div class="error-message">{error}</div>
      {/if}
      
      <!-- Info row above progress bar -->
      <div class="info-row">
        <span class="status-text">
          {loadingStage}
        </span>
        <div class="right-info">
          {#if speedMb && speedMb !== "--"}
            <span class="speed">{speedMb} Mbps</span>
          {/if}
          <span class="percentage">{percentageLabel}</span>
        </div>
      </div>
      
      <!-- Progress bar (100% red when error) -->
      <ProgressBar class={error ? "errored" : ""} percentage={error ? 100 : percentage} />
    </div>

    <!-- Welcome panel -->
    <div class="welcome-panel">
      <h2>Welcome to Asteria Universe!</h2>
      {#key currentTipIndex}
        <p transition:slide={{ duration: 300 }}>{tips[currentTipIndex]}</p>
      {/key}
    </div>
  </div>
</div>

<style lang="scss">
  .fullscreen-container {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    pointer-events: none;
  }

  .top-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1.25rem 1.25rem;
    font-size: 0.6rem;
    color: rgba(255, 255, 255, 0.5);
    pointer-events: auto;
    font-weight: 100;
    .powered-by {
      font-weight: 100;
    }
    
    .build-version {
      font-weight: 100;
    }
  }

  .bottom-content {
    display: flex;
    flex-direction: column;
    gap: 0;
    pointer-events: auto;
  }

  .overlay-block {
    background: transparent;
    padding: 1rem 0 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }

  .info-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 0.75rem;
    padding: 0 1.25rem;
    
    .status-text {
      color: rgba(255, 255, 255, 0.7);
      font-weight: 100;
    }
    
    .right-info {
      display: flex;
      align-items: center;
      gap: 0.75rem;
      
      .speed {
        color: rgba(255, 255, 255, 0.6);
        font-size: 0.7rem;
      }
      
      .percentage {
        color: $text-primary;
        font-weight: 700;
        font-size: 0.85rem;
      }
    }
  }

  .error-message {
    background: rgba(255, 107, 107, 0.15);
    border: 1px solid rgba(255, 107, 107, 0.4);
    border-radius: 0.5rem;
    padding: 0.75rem 1rem;
    margin-bottom: 5.75rem;
    margin-left: 1rem;
    margin-right: 1rem;
    color: $error;
    font-size: 0.8rem;
    text-align: center;
    font-weight: 600;
  }

  .welcome-panel {
    background: rgba(0, 0, 0, 0.9);
    padding: 1rem 1.25rem;
    border-radius: 0 0 12px 12px;
    
    h2 {
      color: $text-primary;
      font-size: 0.9rem;
      font-weight: 600;
      margin: 0 0 0.25rem 0;
      letter-spacing: 0.15px;
    }
    
    p {
      color: rgba(255, 255, 255, 0.6);
      font-size: 0.7rem;
      margin: 0;
      line-height: 1.3;
    }
  }
</style>
