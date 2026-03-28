import type { ReactNode } from "react";

interface EmptyStateProps {
	icon: ReactNode;
	title: string;
	description?: string;
	action?: ReactNode;
}

export function EmptyState({ icon, title, description, action }: EmptyStateProps) {
	return (
		<div className="flex flex-col items-center justify-center py-16 px-4 text-center">
			<div className="text-text-muted mb-3">{icon}</div>
			<h3 className="text-base font-medium text-text-primary mb-1">{title}</h3>
			{description && <p className="text-sm text-text-secondary max-w-xs">{description}</p>}
			{action && <div className="mt-4">{action}</div>}
		</div>
	);
}
