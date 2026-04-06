import type { ReactNode } from "react";

interface CardProps {
	children: ReactNode;
	className?: string;
	padding?: "none" | "sm" | "md" | "lg";
}

const paddingMap = {
	none: "",
	sm: "p-3",
	md: "p-4 sm:p-5",
	lg: "p-5 sm:p-6",
};

export function Card({ children, className = "", padding = "md" }: CardProps) {
	return (
		<div
			className={`bg-surface-2 border border-border rounded-xl ${paddingMap[padding]} ${className}`}
		>
			{children}
		</div>
	);
}

interface CardHeaderProps {
	title: string;
	description?: string;
	action?: ReactNode;
}

export function CardHeader({ title, description, action }: CardHeaderProps) {
	return (
		<div className="flex items-start justify-between mb-4">
			<div>
				<h3 className="text-base font-semibold text-text-primary">{title}</h3>
				{description && <p className="text-sm text-text-secondary mt-0.5">{description}</p>}
			</div>
			{action && <div className="ml-4 shrink-0">{action}</div>}
		</div>
	);
}
