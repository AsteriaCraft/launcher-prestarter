<script lang="ts">
    import "reset-css";
    import { invoke } from "@tauri-apps/api/core";
    import { listen } from "@tauri-apps/api/event";
    import { getCurrentWindow } from "@tauri-apps/api/window";
    import { onMount } from "svelte";

    import "$lib/assets/css/global.scss";

    import { assets } from "$lib/assets";

    import { appConfig, tauriCommands, tauriEvents } from "$lib/config/app";
    import { DownloadTracker } from "$lib/utils/download";
    import type {
        ExtractProgressEvent,
        DownloadProgressEvent,
    } from "$lib/types/events";
    import DownloadBlock from "$lib/components/ui/DownloadBlock.svelte";

    // Application state
    let lastSpeedUpdate = 0;
    let error = "";
    let speedMb: string = "";
    let percentage: number = 0;

    let done = false;
    let running = false;

    // Loading stages
    let loadingStage = "Loading modules...";

    const appWindow = getCurrentWindow();
    const downloadTracker = new DownloadTracker();

    // Функции управления окном
    const minimize = () => appWindow.minimize();
    const close_application = () => invoke(tauriCommands.closeApp);

    // Загрузка
    async function startDownload() {
        try {
            console.log("Инициализация загрузки...");
            error = "";
            done = false;
            running = false;
            downloadTracker.reset();
            speedMb = "";

            const result = await invoke<string>(tauriCommands.startDownload);
            console.log("Download started:", result || "success");
        } catch (err) {
            console.error("Download initialization error:", err);
            error = String(err);
            speedMb = "ERR";
        }
    }

    // Add async/await for error handling
    async function setupListeners() {
        try {
            await listen<DownloadProgressEvent>(
                tauriEvents.downloadProgress,
                (event) => {
                    const now = Date.now();
                    const current = event.payload.downloaded;
                    const total = event.payload.total;

                    // Set loading stage based on event type
                    if (current > 0) {
                        loadingStage = "Loading modules...";
                    }

                    // Update via DownloadTracker
                    const result = downloadTracker.update(current, total);

                    speedMb = result.speed;
                    percentage = result.percentage;

                    if (now - lastSpeedUpdate > 200) {
                        lastSpeedUpdate = now;
                    }
                },
            );

            await listen<ExtractProgressEvent>(
                tauriEvents.extractProgress,
                (event) => {
                    // Set loading stage based on event type
                    if (event.payload.processed > 0) {
                        loadingStage = "Installing modules...";
                    }
                    
                    speedMb = "--";
                    percentage = downloadTracker.percentageCalculation(
                        event.payload.processed,
                        event.payload.total,
                    );

                },
            );
            await listen<string>(tauriEvents.error, (event) => {
                speedMb = "ERR";
                error = event.payload;
            });
            await listen(tauriEvents.running, () => {
                loadingStage = "Starting...";
                running = true;
            });
            await listen(tauriEvents.done, () => {
                done = true;
                close_application();
            });
        } catch (err) {
            console.error("Failed to setup listeners:", err);
        }
    }

    onMount(() => {
        setupListeners();
        setTimeout(startDownload, appConfig.download.initialDelay);
    });
</script>

<svelte:head>
    <title>Asterium Craft</title>
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
</svelte:head>

<div data-tauri-drag-region class="app">
    <div class="noise"></div>
    <div class="layout">
        <DownloadBlock {error} {speedMb} {percentage} {loadingStage} />
    </div>
</div>

<style lang="scss">
    :global(.app) {
        border-radius: 12px;
        width: 100vw;
        height: 100vh;
        display: flex;
        justify-content: center;
        align-items: center;
        overflow: hidden;
        position: relative;
        background:
            url("$lib/assets/images/back.jpg");
        background-size: cover;
        background-repeat: no-repeat;
        background-position: center;
        font-family: "Inter", "Kharkiv", sans-serif;
        color: $text-primary;
    }
    .layout {
        width: 100%;
        height: 100%;
        position: relative;
    }
</style>
