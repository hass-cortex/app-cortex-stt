import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useKeys, useRevokeKey } from "@/hooks/use-keys";
import { formatRelativeTime, formatTimestamp } from "@/lib/format";
import { Key, Plus, Trash2 } from "lucide-react";

interface KeyListProps {
	onGenerate: () => void;
}

export function KeyList({ onGenerate }: KeyListProps) {
	const { data: keys, isLoading, error } = useKeys();
	const revokeMutation = useRevokeKey();
	const { toast } = useToast();

	const handleRevoke = (id: string, name: string) => {
		if (!window.confirm(`Revoke API key "${name}"? This cannot be undone.`)) return;
		revokeMutation.mutate(id, {
			onSuccess: () => toast("API key revoked", "success"),
			onError: (err) => toast(`Failed: ${err.message}`, "error"),
		});
	};

	if (isLoading) {
		return (
			<div className="flex justify-center py-16">
				<Spinner size="lg" />
			</div>
		);
	}

	if (error) {
		return (
			<EmptyState
				icon={<Key size={40} />}
				title="Failed to load API keys"
				description={error.message}
			/>
		);
	}

	return (
		<Card>
			<CardHeader
				title="API Keys"
				description="Keys authenticate HTTP API requests."
				action={
					<Button size="sm" icon={<Plus size={14} />} onClick={onGenerate}>
						Generate Key
					</Button>
				}
			/>

			{!keys || keys.length === 0 ? (
				<EmptyState
					icon={<Key size={32} />}
					title="No API keys"
					description="Generate a key to authenticate HTTP API requests."
					action={
						<Button size="sm" icon={<Plus size={14} />} onClick={onGenerate}>
							Generate Key
						</Button>
					}
				/>
			) : (
				<div className="overflow-x-auto">
					<table className="w-full text-sm">
						<thead>
							<tr className="border-b border-border">
								<th className="text-left py-2 px-3 text-xs font-medium text-text-muted uppercase">
									Name
								</th>
								<th className="text-left py-2 px-3 text-xs font-medium text-text-muted uppercase">
									Key
								</th>
								<th className="text-left py-2 px-3 text-xs font-medium text-text-muted uppercase hidden sm:table-cell">
									Created
								</th>
								<th className="text-left py-2 px-3 text-xs font-medium text-text-muted uppercase hidden md:table-cell">
									Last Used
								</th>
								<th className="text-right py-2 px-3 text-xs font-medium text-text-muted uppercase">
									Actions
								</th>
							</tr>
						</thead>
						<tbody>
							{keys.map((key) => (
								<tr key={key.id} className="border-b border-border/50 hover:bg-surface-3/50">
									<td className="py-2.5 px-3 font-medium text-text-primary">{key.name}</td>
									<td className="py-2.5 px-3">
										<code className="text-xs text-text-muted font-mono">****{key.last4}</code>
									</td>
									<td className="py-2.5 px-3 text-text-secondary hidden sm:table-cell">
										{formatTimestamp(key.created_at)}
									</td>
									<td className="py-2.5 px-3 text-text-muted hidden md:table-cell">
										{key.last_used_at ? formatRelativeTime(key.last_used_at) : "Never"}
									</td>
									<td className="py-2.5 px-3 text-right">
										<Button
											variant="ghost"
											size="sm"
											icon={<Trash2 size={14} />}
											onClick={() => handleRevoke(key.id, key.name)}
											loading={revokeMutation.isPending}
											className="text-error hover:text-error"
										/>
									</td>
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}
		</Card>
	);
}
