import { HardwareCard } from "@/components/dashboard/hardware-card";
import { MetricsCard } from "@/components/dashboard/metrics-card";
import { ModelStatusCard } from "@/components/dashboard/model-status-card";

export function DashboardPage() {
	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">Dashboard</h1>
				<p className="text-sm text-text-secondary mt-1">System overview and real-time metrics</p>
			</div>

			<div className="grid grid-cols-1 md:grid-cols-2 gap-4">
				<div className="md:col-span-2">
					<HardwareCard />
				</div>
				<ModelStatusCard />
				<MetricsCard />
			</div>
		</div>
	);
}
