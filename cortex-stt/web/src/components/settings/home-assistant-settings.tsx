import { useMutation } from "@tanstack/react-query";
import { Megaphone } from "lucide-react";
import { type AnnounceResponse, announceDiscovery } from "@/api/discovery";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { useMutationToast } from "@/hooks/use-mutation-toast";

export function HomeAssistantSettings() {
	const announceMutation = useMutation<AnnounceResponse, Error, void>({
		mutationFn: announceDiscovery,
	});
	const announce = useMutationToast(announceMutation, {
		success: (data) => `Announced ${data.host}:${data.port} to Supervisor`,
		error: "Discovery announce failed",
	});

	return (
		<Card>
			<CardHeader title="Home Assistant" description="Discovery and integration controls" />
			<div className="space-y-3">
				<p className="text-xs text-text-muted">
					Re-publish this app's host, port and API key to the Home Assistant Supervisor. Use this
					when the integration was installed after the app, or when the discovery card was dismissed
					and you want it back.
				</p>
				<Button
					icon={<Megaphone size={14} />}
					onClick={() => announce()}
					loading={announceMutation.isPending}
				>
					Re-announce to Home Assistant
				</Button>
			</div>
		</Card>
	);
}
