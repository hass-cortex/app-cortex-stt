import { Check, Copy, Eye, EyeOff, Key, Lock, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { useConfirm } from "@/components/ui/confirm";
import { EmptyState } from "@/components/ui/empty-state";
import { Hint } from "@/components/ui/hint";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useKeys, useRevokeKey } from "@/hooks/use-keys";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { copyToClipboard } from "@/lib/clipboard";
import { formatRelativeTime } from "@/lib/format";
import { formatTimestamp } from "@/utils/time";

interface KeyListProps {
	onGenerate: () => void;
}

export function KeyList({ onGenerate }: KeyListProps) {
	const { data: keys, isLoading, error } = useKeys();
	const revokeMutation = useRevokeKey();
	const { toast } = useToast();
	const confirm = useConfirm();
	const runRevoke = useMutationToast(revokeMutation, { success: "API key revoked" });
	const [visibleKeys, setVisibleKeys] = useState<Set<string>>(new Set());
	const [copiedKey, setCopiedKey] = useState<string | null>(null);

	const handleRevoke = async (id: string, name: string) => {
		const ok = await confirm({
			title: `Revoke “${name}”?`,
			body: "Anything still authenticating with this key starts failing immediately. Other keys keep working.",
			confirmLabel: "Revoke key",
			destructive: true,
		});
		if (ok) runRevoke(id);
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
			await copyToClipboard(key);
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
				title="Keys"
				description="one key per caller — revoking one then silences only that caller"
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
					<table className="w-full min-w-[680px] text-[12.5px]">
						<thead>
							<tr className="border-b border-border">
								<th className="num text-left py-2 px-3 text-[10px] tracking-[0.06em] text-text-faint uppercase">
									Name
								</th>
								<th className="num text-left py-2 px-3 text-[10px] tracking-[0.06em] text-text-faint uppercase">
									Key
								</th>
								<th className="num text-left py-2 px-3 text-[10px] tracking-[0.06em] text-text-faint uppercase hidden sm:table-cell">
									Created
								</th>
								<th className="num text-left py-2 px-3 text-[10px] tracking-[0.06em] text-text-faint uppercase hidden md:table-cell">
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
									<tr
										key={key.id}
										className="border-b border-border-soft last:border-0 hover:bg-surface-3/40"
									>
										<td className="py-2.5 px-3 font-medium text-text-primary">
											<div className="flex items-center gap-1.5">
												<span>{key.name}</span>
												{key.system && (
													<Hint
														label="Managed"
														content="Managed by the addon — edit via the Configuration tab."
														width={240}
														className="gap-0.5 px-1.5 py-0.5 text-[10px] uppercase tracking-wide rounded bg-surface-3 text-text-muted"
													>
														<Lock size={9} />
														Managed
													</Hint>
												)}
											</div>
										</td>
										<td className="py-2.5 px-3">
											<div className="flex items-center gap-1.5">
												<code className="num text-[11.5px] text-text-muted select-all">
													{isVisible && key.key ? key.key : `****${key.last4}`}
												</code>
												{key.key && (
													<>
														<button
															type="button"
															onClick={() => toggleVisibility(key.id)}
															className="p-1 text-text-muted hover:text-text-secondary rounded transition-colors"
															aria-label={isVisible ? "Hide key" : "Show key"}
														>
															{isVisible ? <EyeOff size={13} /> : <Eye size={13} />}
														</button>
														<button
															type="button"
															onClick={() => handleCopy(key.id, key.key)}
															className="p-1 text-text-muted hover:text-text-secondary rounded transition-colors"
															aria-label="Copy key"
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
										<td className="num py-2.5 px-3 text-[11.5px] text-text-secondary hidden sm:table-cell">
											{formatTimestamp(key.created_at)}
										</td>
										<td className="num py-2.5 px-3 text-[11.5px] hidden md:table-cell">
											{key.last_used_at ? (
												<span className="text-text-secondary">
													{formatRelativeTime(key.last_used_at)}
												</span>
											) : (
												<span className="text-text-faint">never used</span>
											)}
										</td>
										<td className="py-2.5 px-3 text-right">
											{!key.system && (
												<Button
													variant="ghost"
													size="sm"
													icon={<Trash2 size={14} />}
													onClick={() => handleRevoke(key.id, key.name)}
													loading={revokeMutation.isPending}
													className="text-error hover:text-error"
												/>
											)}
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
