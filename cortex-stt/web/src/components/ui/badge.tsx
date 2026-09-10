import type { ReactNode } from "react";

type BadgeVariant = "default" | "success" | "warning" | "error" | "info" | "accent";

interface BadgeProps {
	children: ReactNode;
	variant?: BadgeVariant;
	className?: string;
}

const variantClasses: Record<BadgeVariant, string> = {
	default: "bg-surface-3 text-text-secondary",
	success: "bg-success-wash text-success",
	warning: "bg-warning-wash text-warning",
	error: "bg-error-wash text-error",
	info: "bg-surface-3 text-info",
	accent: "bg-accent-wash text-accent-ink",
};

export function Badge({ children, variant = "default", className = "" }: BadgeProps) {
	return (
		<span
			className={`num inline-flex items-center gap-1 px-[7px] py-0.5 rounded-[4px] text-[10.5px] whitespace-nowrap ${variantClasses[variant]} ${className}`}
		>
			{children}
		</span>
	);
}
