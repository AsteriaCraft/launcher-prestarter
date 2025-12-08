<script lang="ts">
  import { onDestroy } from "svelte";

  export let percentage: number = 0;

  onDestroy(() => (percentage = 0));
</script>

<div class={"progress-bar " + ($$restProps.class || "")}>
  <div style="width: {percentage}%; height: 100%;"></div>
</div>

<style lang="scss">
  .progress-bar {
    width: 100%;
    height: 6px;
    background: #111727;
    border: none;
    overflow: hidden;
    position: relative;
    > div {
      background: $progressbar;
      transition: width 0.5s cubic-bezier(0.22, 0.61, 0.36, 1);
    }
    &.errored {
      background: rgba(255, 107, 107, 0.1);
      border-color: rgba(255, 107, 107, 0.3);
      > div {
        background: $error;
      }
    }
  }
</style>
