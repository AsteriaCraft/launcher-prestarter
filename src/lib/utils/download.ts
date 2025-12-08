import { appConfig } from '$lib/config/app';

export class DownloadTracker {
    private lastProgress = 0;
    private lastTimestamp = Date.now();
    private speedSamples: number[] = [];
    private lastDisplayedSpeed = 0;
    private lastUpdateTime = 0;
    private readonly updateInterval = 200;

    update(currentProgress: number, totalBytes: number): { speed: string; percentage: number } {
        const now = Date.now();

        // Рассчитываем процент сразу
        const newPercentage = this.percentageCalculation(currentProgress, totalBytes);

        // Пропускаем обновления, если не прошло достаточно времени
        if (now - this.lastUpdateTime < this.updateInterval) {
            return {
                speed: this.formatSpeed(this.lastDisplayedSpeed),
                percentage: newPercentage
            };
        }

        // Расчет скорости
        const elapsedMs = now - this.lastTimestamp;
        if (elapsedMs > 0) {
            const bytesDownloaded = currentProgress - this.lastProgress;
            const secondsElapsed = elapsedMs / 1000;
            const speedBps = bytesDownloaded / secondsElapsed;
            const speedMbps = (speedBps * 8) / (1024 * 1024);

            this.speedSamples.push(speedMbps);
            if (this.speedSamples.length > appConfig.download.speedSamples) {
                this.speedSamples.shift();
            }

            const sum = this.speedSamples.reduce((a, b) => a + b, 0);
            const averageSpeed = sum / this.speedSamples.length;
            const smoothedSpeed = this.lastDisplayedSpeed * 0.7 + averageSpeed * 0.3;

            this.lastDisplayedSpeed = +smoothedSpeed.toFixed(1);
        }

        this.lastProgress = currentProgress;
        this.lastTimestamp = now;
        this.lastUpdateTime = now;

        return {
            speed: this.formatSpeed(this.lastDisplayedSpeed),
            percentage: newPercentage
        };
    }

    reset() {
        this.lastProgress = 0;
        this.lastTimestamp = Date.now();
        this.speedSamples = [];
    }

    private formatSpeed(speed: number): string {
        return speed > 100
            ? Math.round(speed).toString()
            : speed.toString();
    }
    formatTotalLabel(totalBytes: number): string {
        return (totalBytes / 1000 / 1000).toFixed(0) + " MB";
    }
    percentageCalculation(currentProgress: number, totalBytes: number): number {
        return totalBytes > 0
            ? Math.min((currentProgress / totalBytes) * 100, 100)
            : 0;
    }
}