import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useKeys, useRevokeKey } from "@/hooks/use-keys";
import { formatRelativeTime, formatTimestamp } from "@/lib/format";
import { Check, Copy, Eye, EyeOff, Key, Plus, Trash2 } from "lucide-react";
import { useState } from "react";

interface KeyListProps {
	onGenerate: () => void;
}

export function KeyList({ onGenerate }: KeyListProps) {
	const { data: keys, isLoading, error } = useKeys();
	const revokeMutation = useRevokeKey();
	const { toast } = useToast();
	const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());
	const [copiedKey, setCopiedKey] = useState<string | null>(null);

	const handleRevoke = (id: string, name: string) => {
		if (!window.confirm(`Revoke API key "${name}"? This cannot be undone.`)) return;
		revokeMutation.mutate(id, {
			onSuccess: () => toast("API key revoked", "success"),
			onError: (err) => toast(`Failed: ${err.message}`, "error"),
		});
	};

	const toggleVisibility = (id: string) => {
		setVisibleKeys((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	};

	const handleCopy = async (id: string, key: string) => {
		try {
			await navigator.clipboard.writeText(key);
			setCopiedKey(id);
			setTimeout(() => setCopiedKey(null), 2000);
		} catch {
			toast("Failed to copy to clipboard", "error");
		}
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
							{keys.map((key) => {
								const isVisible = visibleKeys.has(key.id);
								const isCopied = copiedKey === key.id;
								return (
									<tr key={key.id} className="border-b border-border/50 hover:bg-surface-3/50">
										<td className="py-2.5 px-3 font-medium text-text-primary">{key.name}</td>
										<td className="py-2.5 px-3">
											<div className="flex items-center gap-1.5">
												<code className="text-xs text-text-muted font-mono select-all">
													{isVisible && key.key ? key.key : `****${key.last4}`}
												</code>
												{key.key && (
													<>
														<button
															type="button"
															onClick={() => toggleVisibility(key.id)}
															className="p-1 text-text-muted hover:text-text-secondary rounded transition-colors"
															title={isVisible ? "Hide key" : "Show key"}
														>
															{isVisible ? <EyeOff size={13} /> : <Eye size={13} />}
														</button>
														<button
															type="button"
															onClick={() => handleCopy(key.id, key.key)}
															className="p-1 text-text-muted hover:text-text-secondary rounded transition-colors"
															title="Copy key"
														>
															{isCopied ? (
																<Check size={13} className="text-success" />
															) : (
																<Copy size={13} />
															)}
														</button>
													</>
												)}
											</div>
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
								);
							})}
						</tbody>
					</table>
				</div>
			)}
		</Card>
	);
}
