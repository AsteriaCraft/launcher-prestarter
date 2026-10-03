<script lang="ts">
  interface Props {
    percentage?: number;
    indeterminate?: boolean;
    errored?: boolean;
  }

  let { percentage = 0, indeterminate = false, errored = false }: Props = $props();
</script>

<div
  class="progress-bar"
  class:errored
  class:indeterminate={indeterminate && !errored}
  role="progressbar"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={indeterminate ? undefined : Math.round(percentage)}
>
  <div class="fill" style="width: {errored ? 100 : indeterminate ? 35 : percentage}%"></div>
</div>

<style lang="scss">
  .progress-bar {
    width: 100%;
    height: 6px;
    background: #111727;
    overflow: hidden;
    position: relative;

    .fill {
      height: 100%;
      background: $progressbar;
      transition: width 0.5s cubic-bezier(0.22, 0.61, 0.36, 1);
    }

    &.indeterminate .fill {
      position: absolute;
      animation: slide 1.4s ease-in-out infinite;
      transition: none;
    }

    &.errored {
      background: rgba(255, 107, 107, 0.1);
      .fill {
        background: $error;
      }
    }
  }

  @keyframes slide {
    from {
      left: -35%;
    }
    to {
      left: 100%;
    }
  }
</style>
