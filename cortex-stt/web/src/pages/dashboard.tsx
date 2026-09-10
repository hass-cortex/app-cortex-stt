import { Mic } from "lucide-react";
import { Link } from "react-router";
import { LatencyHistogram } from "@/components/charts/latency-histogram";
import { ThroughputChart } from "@/components/charts/throughput-chart";
import { EngineCard } from "@/components/dashboard/engine-card";
import { Kpi } from "@/components/dashboard/kpi";
import { RecentCard } from "@/components/dashboard/recent-card";
import { StorageCard } from "@/components/dashboard/storage-card";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useDashboardStats } from "@/hooks/use-stats";
import { useMetrics, useSystemInfo } from "@/hooks/use-system";
import { ROUTES } from "@/lib/constants";
import { formatDuration, formatNumber } from "@/lib/format";

function hardwareLine(
	cpuCount: number | undefined,
	avx2: boolean | undefined,
	cuda: boolean | undefined,
): string {
	const parts: string[] = [];
	if (cpuCount) parts.push(`${cpuCount} CPU cores`);
	if (avx2) parts.push("AVX2");
	parts.push(cuda ? "CUDA available" : "no CUDA");
	return parts.join(" · ");
}

export function DashboardPage() {
	const { data: metrics } = useMetrics();
	const { data: system } = useSystemInfo();
	const stats = useDashboardStats();

	return (
		<div className="flex flex-col gap-4">
			<div className="flex items-end justify-between gap-4">
				<div>
					<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">
						Dashboard
					</h1>
					<p className="text-[12.5px] text-text-secondary mt-1">
						{hardwareLine(system?.cpu_count, system?.has_avx2, system?.cuda_available)}
					</p>
				</div>
				<Link to={ROUTES.TRANSCRIBE}>
					<Button icon={<Mic size={14} strokeWidth={1.8} />}>Transcribe something</Button>
				</Link>
			</div>

			<div className="grid grid-cols-1 sm:grid-cols-2 xl:grid-cols-4 gap-4">
				<Kpi
					label="TRANSCRIPTIONS TODAY"
					value={formatNumber(metrics?.today_transcriptions ?? 0)}
					sub={`${formatNumber(metrics?.total_transcriptions ?? 0)} all time`}
				/>
				{/* One window per tile: mixing an all-time average with a 24-hour
				    p50 in the same box invites reading them against each other. */}
				<Kpi
					label="MEDIAN LATENCY · 24 H"
					value={stats.latency.p50 === null ? "—" : Math.round(stats.latency.p50).toString()}
					unit={stats.latency.p50 === null ? undefined : "ms"}
					sub={
						stats.latency.p50 === null
							? `no requests · ${Math.round(metrics?.avg_inference_ms ?? 0)} ms all-time mean`
							: `p95 ${formatDuration(stats.latency.p95 ?? 0)} · max ${formatDuration(stats.latency.max ?? 0)}`
					}
				/>
				<Kpi
					label="REAL-TIME FACTOR"
					value={stats.rtf === null ? "—" : stats.rtf.toFixed(2)}
					unit={stats.rtf === null ? undefined : "×"}
					sub={
						(metrics?.today_audio_duration_ms ?? 0) > 0
							? `${formatDuration(metrics?.today_audio_duration_ms ?? 0)} of audio today`
							: "no audio yet today"
					}
				/>
				<Kpi
					label="ERRORS TODAY"
					value={formatNumber(metrics?.today_error_count ?? 0)}
					tone={(metrics?.today_error_count ?? 0) > 0 ? "error" : "default"}
					sub={`${formatNumber(metrics?.error_count ?? 0)} since first run`}
				/>
			</div>

			<div className="flex flex-col xl:flex-row gap-4">
				<Card className="xl:w-2/3 min-w-0">
					<CardHeader
						title="Throughput"
						description="transcriptions per hour · last 24 h"
						action={
							stats.peak && (
								<span className="num text-[11px] text-text-muted">
									peak {stats.peak.count} at {String(stats.peak.start.getHours()).padStart(2, "0")}
									:00
								</span>
							)
						}
					/>
					<div className="mt-3">
						{stats.isLoading ? (
							<div className="flex justify-center py-12">
								<Spinner />
							</div>
						) : (
							<ThroughputChart buckets={stats.hours} />
						)}
					</div>
				</Card>

				<Card className="flex-1 min-w-0 flex flex-col">
					<CardHeader
						title="Inference latency"
						description={`${stats.latency.samples} requests · last 24 h`}
					/>
					<div className="mt-3">
						<LatencyHistogram summary={stats.latency} />
					</div>
					<div className="num mt-auto pt-3 border-t border-border-soft flex gap-5 text-[11px] text-text-muted">
						<span>
							p50{" "}
							<span className="text-text-primary">
								{stats.latency.p50 === null ? "—" : formatDuration(stats.latency.p50)}
							</span>
						</span>
						<span>
							p95{" "}
							<span className="text-text-primary">
								{stats.latency.p95 === null ? "—" : formatDuration(stats.latency.p95)}
							</span>
						</span>
						<span>
							max{" "}
							<span className="text-text-primary">
								{stats.latency.max === null ? "—" : formatDuration(stats.latency.max)}
							</span>
						</span>
					</div>
				</Card>
			</div>

			<div className="flex flex-col xl:flex-row gap-4 items-stretch">
				<div className="xl:w-2/3 min-w-0 flex">
					<RecentCard records={stats.recent} />
				</div>
				<div className="flex-1 min-w-0 flex flex-col gap-4">
					<EngineCard />
					<StorageCard />
				</div>
			</div>
		</div>
	);
}
