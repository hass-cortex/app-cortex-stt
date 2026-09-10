import { useEffect, useMemo, useRef, useState } from "react";
import { DangerZone } from "@/components/settings/danger-zone";
import { DefaultModelSettings } from "@/components/settings/default-model-settings";
import { EngineSettings } from "@/components/settings/engine-settings";
import { HomeAssistantSettings } from "@/components/settings/home-assistant-settings";
import { RetentionSettings } from "@/components/settings/retention-settings";
import { TimezoneSettings } from "@/components/settings/timezone-settings";
import { Spinner } from "@/components/ui/spinner";
import { useSettings } from "@/hooks/use-settings";
import { describeRetention } from "@/lib/retention";

interface Section {
	id: string;
	label: string;
	note: string;
	element: React.ReactNode;
}

export function SettingsPage() {
	const { data: settings, isLoading } = useSettings();
	const [active, setActive] = useState("default-model");
	const navRef = useRef<HTMLElement>(null);

	const sections: Section[] = useMemo(
		() => [
			{
				id: "default-model",
				label: "Default model",
				note: settings?.default_model ?? "not set",
				element: <DefaultModelSettings />,
			},
			{
				id: "engine",
				label: "Engine",
				note: settings
					? `pool ${settings.pool_size} · max ${settings.max_loaded_models} loaded`
					: "",
				element: <EngineSettings />,
			},
			{
				id: "retention",
				label: "Retention",
				note: settings ? `rows ${describeRetention(settings.record_retention)}` : "",
				element: <RetentionSettings />,
			},
			{
				id: "timezone",
				label: "Timezone",
				note: settings?.timezone ?? "",
				element: <TimezoneSettings />,
			},
			{
				id: "home-assistant",
				label: "Home Assistant",
				note: "discovery",
				element: <HomeAssistantSettings />,
			},
			{ id: "danger-zone", label: "Danger zone", note: "", element: <DangerZone /> },
		],
		[settings],
	);

	// Highlight whichever section the reader is actually looking at, rather
	// than whichever one was clicked last. The observer's root is the
	// viewport, so everything above the scroll container is already out of
	// view; stacked below lg, the sticky strip hides a band below that too.
	useEffect(() => {
		const scroller = navRef.current?.closest("main");
		const stacked = !window.matchMedia("(min-width: 1024px)").matches;
		const strip = stacked
			? Number.parseFloat(getComputedStyle(scroller ?? document.body).paddingTop) +
				(navRef.current?.offsetHeight ?? 0)
			: 0;
		const hidden = (scroller?.getBoundingClientRect().top ?? 0) + strip;
		const observer = new IntersectionObserver(
			(entries) => {
				const visible = entries
					.filter((e) => e.isIntersecting)
					.sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
				if (visible?.target.id) setActive(visible.target.id);
			},
			{ rootMargin: `-${hidden}px 0px -60% 0px` },
		);
		for (const section of sections) {
			const node = document.getElementById(section.id);
			if (node) observer.observe(node);
		}
		return () => observer.disconnect();
	}, [sections]);

	if (isLoading) {
		return (
			<div className="flex justify-center py-16">
				<Spinner size="lg" />
			</div>
		);
	}

	return (
		<div className="flex flex-col gap-4">
			<div>
				<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">Settings</h1>
				<p className="text-[12.5px] text-text-secondary mt-1">
					Changes apply as soon as they are saved. The retention sweep runs hourly in the configured
					timezone.
				</p>
			</div>

			<div className="flex flex-col lg:flex-row gap-5">
				{/* Sticky at every width. Stacked on a phone it is a strip the
				    sections scroll under, so it carries the page's own ground and a
				    rule to sit on. The shadow extends that ground upward over the
				    scroll container's top padding, which sticky leaves above it. */}
				<nav
					ref={navRef}
					className="sticky top-0 z-10 lg:w-52 shrink-0 lg:self-start flex lg:flex-col gap-0.5 overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden bg-surface-0 -mx-4 px-4 sm:-mx-7 sm:px-7 py-1.5 border-b border-border shadow-[0_-40px_0_0_var(--surface-0)] lg:mx-0 lg:px-0 lg:py-0 lg:border-0 lg:shadow-none"
				>
					{sections.map((section) => (
						<a
							key={section.id}
							href={`#${section.id}`}
							className={`flex flex-col gap-0.5 px-3 py-2.5 rounded-md transition-colors whitespace-nowrap ${
								active === section.id
									? "bg-accent-wash text-text-primary shadow-[inset_2px_0_0_var(--accent)]"
									: "text-text-secondary hover:bg-surface-3"
							}`}
						>
							<span className={`text-[12.5px] ${active === section.id ? "font-semibold" : ""}`}>
								{section.label}
							</span>
							{/* A phone gets the strip, not the notes: a second line there
							    costs more screen than it explains. */}
							{section.note && (
								<span className="num hidden lg:block text-[10.5px] text-text-faint truncate">
									{section.note}
								</span>
							)}
						</a>
					))}
				</nav>

				<div className="flex-1 min-w-0 flex flex-col gap-4">
					{sections.map((section) => (
						<section
							key={section.id}
							id={section.id}
							className="scroll-mt-[68px] sm:scroll-mt-[76px] lg:scroll-mt-4"
						>
							{section.element}
						</section>
					))}
				</div>
			</div>
		</div>
	);
}
