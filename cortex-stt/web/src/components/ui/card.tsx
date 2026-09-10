import type { ReactNode } from "react";

interface CardProps {
	children: ReactNode;
	className?: string;
	padding?: "none" | "sm" | "md" | "lg";
}

const paddingMap = {
	none: "",
	sm: "p-3.5",
	md: "p-4",
	lg: "px-[18px] py-4",
};

export function Card({ children, className = "", padding = "lg" }: CardProps) {
	return (
		<div
			className={`bg-surface-2 border border-border rounded-[10px] ${paddingMap[padding]} ${className}`}
		>
			{children}
		</div>
	);
}

interface CardHeaderProps {
	title: string;
	/** Sub-line under the title. Set in the mono face — it usually carries
	 *  counts, units or model ids rather than prose. */
	description?: ReactNode;
	action?: ReactNode;
}

export function CardHeader({ title, description, action }: CardHeaderProps) {
	return (
		<div className="flex items-baseline justify-between gap-4">
			<div className="min-w-0">
				<h3 className="text-[13.5px] font-semibold text-text-primary">{title}</h3>
				{description && <p className="num text-[11px] text-text-muted mt-[3px]">{description}</p>}
			</div>
			{action && <div className="shrink-0">{action}</div>}
		</div>
	);
}
