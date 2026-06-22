use leptos::prelude::*;

use crate::{app::current_user, islands::ThemeToggle};

#[component]
pub(crate) fn LandingPage() -> impl IntoView {
    let auth = current_user();
    let is_authenticated = auth.is_some();

    view! {
        <div class="min-h-screen bg-gradient-to-b from-paper via-panel-bg to-paper text-ink transition-colors duration-200">
            // Navigation Header
            <header class="sticky top-0 z-50 backdrop-blur-md bg-paper/80 border-b border-border transition-all">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="flex items-center justify-between h-16 sm:h-20">
                        <div class="flex items-center gap-3">
                            <span class="flex items-center justify-center w-10 h-10 rounded-xl bg-gradient-to-tr from-accent to-accent/80 text-white font-extrabold text-xl shadow-md shadow-accent/20">
                                "P"
                            </span>
                            <span class="text-xl font-black tracking-tight text-ink">"PayPlan"</span>
                        </div>
                        <nav class="hidden md:flex items-center gap-8 text-sm font-semibold text-muted">
                            <a href="#engine" class="hover:text-accent transition-colors">"Core Engine"</a>
                            <a href="#mechanics" class="hover:text-accent transition-colors">"Compensation Mechanics"</a>
                            <a href="#how-it-works" class="hover:text-accent transition-colors">"How It Works"</a>
                            <a href="#benefits" class="hover:text-accent transition-colors">"Benefits"</a>
                        </nav>
                        <div class="flex items-center gap-4">
                            <ThemeToggle/>
                            {if is_authenticated {
                                view! {
                                    <div class="flex items-center gap-3">
                                        <a
                                            href="/dashboard"
                                            class="inline-flex items-center justify-center px-4 py-2 text-sm font-bold text-white bg-accent hover:bg-accent/90 rounded-lg transition-all shadow-sm hover:shadow-md"
                                        >
                                            "Go to Console"
                                        </a>
                                        <form method="post" action="/logout" class="inline">
                                            <button
                                                type="submit"
                                                class="inline-flex items-center justify-center px-4 py-2 text-sm font-semibold text-muted hover:text-ink hover:bg-ink/5 rounded-lg transition-all"
                                            >
                                                "Sign Out"
                                            </button>
                                        </form>
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <a
                                        href="/login"
                                        class="inline-flex items-center justify-center px-5 py-2.5 text-sm font-bold text-white bg-accent hover:bg-accent/90 rounded-xl transition-all shadow-md shadow-accent/10 hover:shadow-lg hover:shadow-accent/20 hover:-translate-y-0.5"
                                    >
                                        "Sign In"
                                    </a>
                                }.into_any()
                            }}
                        </div>
                    </div>
                </div>
            </header>

            // Hero Section
            <section class="relative overflow-hidden py-20 sm:py-32">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 relative z-10">
                    <div class="text-center max-w-4xl mx-auto">
                        <div class="inline-flex items-center gap-2 px-3 py-1.5 rounded-full bg-accent/10 border border-accent/20 text-accent font-bold text-xs uppercase tracking-widest mb-6 sm:mb-8 animate-fade-in">
                            <span class="w-1.5 h-1.5 rounded-full bg-accent animate-ping"></span>
                            "Developer-First MLM Infrastructure"
                        </div>
                        <h1 class="text-4xl sm:text-6xl font-black tracking-tight text-ink leading-none mb-6">
                            "The Configurable MLM "
                            <span class="text-transparent bg-clip-text bg-gradient-to-r from-accent to-accent/80">
                                "Payplan Engine"
                            </span>
                            " Built in Rust"
                        </h1>
                        <p class="text-lg sm:text-xl text-muted font-medium leading-relaxed max-w-2xl mx-auto mb-10">
                            "Design, validate, and execute complex compensation models with absolute mathematical certainty. Fully integrated Matrix, Flushline, and Pot Bonus stacks running on transactionally secure, audit-ready ledgers."
                        </p>
                        <div class="flex flex-col sm:flex-row items-center justify-center gap-4">
                            {if is_authenticated {
                                view! {
                                    <a
                                        href="/dashboard"
                                        class="w-full sm:w-auto inline-flex items-center justify-center px-8 py-4 text-base font-bold text-white bg-accent hover:bg-accent/90 rounded-xl transition-all shadow-lg shadow-accent/20 hover:shadow-xl hover:shadow-accent/30 hover:-translate-y-0.5"
                                    >
                                        "Access Administration Console"
                                    </a>
                                }.into_any()
                            } else {
                                view! {
                                    <a
                                        href="/login"
                                        class="w-full sm:w-auto inline-flex items-center justify-center px-8 py-4 text-base font-bold text-white bg-accent hover:bg-accent/90 rounded-xl transition-all shadow-lg shadow-accent/20 hover:shadow-xl hover:shadow-accent/30 hover:-translate-y-0.5"
                                    >
                                        "Get Started Now"
                                    </a>
                                }.into_any()
                            }}
                            <a
                                href="#engine"
                                class="w-full sm:w-auto inline-flex items-center justify-center px-8 py-4 text-base font-bold text-ink bg-panel-bg border border-border hover:bg-paper rounded-xl transition-all shadow-sm hover:shadow-md"
                            >
                                "Explore Architecture"
                            </a>
                        </div>
                    </div>
                </div>
                // Decorative background grids
                <div class="absolute inset-0 bg-[linear-gradient(to_right,var(--color-border)_1px,transparent_1px),linear-gradient(to_bottom,var(--color-border)_1px,transparent_1px)] bg-[size:4rem_4rem] [mask-image:radial-gradient(ellipse_60%_50%_at_50%_0%,#000_70%,transparent_100%)] -z-10 opacity-60"></div>
            </section>

            // Payplan Engine Architecture Section
            <section id="engine" class="py-20 sm:py-28 border-t border-border bg-panel-bg">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="grid grid-cols-1 lg:grid-cols-12 gap-12 sm:gap-16 items-center">
                        <div class="lg:col-span-5">
                            <span class="text-accent font-extrabold text-sm uppercase tracking-wider">
                                "Unified Model Architecture"
                            </span>
                            <h2 class="text-3xl sm:text-4xl font-black text-ink tracking-tight mt-2 mb-6">
                                "An MLM Engine Configured to Your Business Model"
                            </h2>
                            <p class="text-base sm:text-lg text-muted leading-relaxed mb-6">
                                "The engine decouples core catalog items from user enrollments, enabling operators to scale various package types, billing methods, and reward programs without restructuring codebase pipelines."
                            </p>
                            <ul class="space-y-4">
                                <li class="flex items-start gap-3">
                                    <span class="flex-shrink-0 w-5 h-5 rounded-full bg-accent/10 text-accent flex items-center justify-center text-xs font-bold mt-1">"✓"</span>
                                    <div>
                                        <strong class="text-ink">"Multi-Tenant Scope"</strong>
                                        <p class="text-muted text-sm mt-0.5">"Isolate user catalogs, purchases, billing setups, and payouts per tenant or unified company."</p>
                                    </div>
                                </li>
                                <li class="flex items-start gap-3">
                                    <span class="flex-shrink-0 w-5 h-5 rounded-full bg-accent/10 text-accent flex items-center justify-center text-xs font-bold mt-1">"✓"</span>
                                    <div>
                                        <strong class="text-ink">"Flexible Billing Integration"</strong>
                                        <p class="text-muted text-sm mt-0.5">"Configure products and services on either one-time or recurring intervals (daily, weekly, monthly, etc.) with trials."</p>
                                    </div>
                                </li>
                                <li class="flex items-start gap-3">
                                    <span class="flex-shrink-0 w-5 h-5 rounded-full bg-accent/10 text-accent flex items-center justify-center text-xs font-bold mt-1">"✓"</span>
                                    <div>
                                        <strong class="text-ink">"Immutable Payplan Stacks"</strong>
                                        <p class="text-muted text-sm mt-0.5">"Package compensation rules are versioned and immutable once transactions exist to ensure absolute auditability."</p>
                                    </div>
                                </li>
                            </ul>
                        </div>
                        <div class="lg:col-span-7 bg-[#10291f] text-[#d7e9df] p-6 sm:p-8 rounded-2xl shadow-xl font-mono text-xs overflow-x-auto">
                            <div class="flex items-center justify-between pb-4 mb-6 border-b border-[#1d4837]">
                                <div class="flex items-center gap-1.5">
                                    <span class="w-3 h-3 rounded-full bg-red-500"></span>
                                    <span class="w-3 h-3 rounded-full bg-yellow-500"></span>
                                    <span class="w-3 h-3 rounded-full bg-green-500"></span>
                                </div>
                                <span class="text-[#527064] font-bold">"payplan_core :: reward_ledger"</span>
                            </div>
                            <span class="text-[#2ea46f] block mb-2">"// Audit-grade reward ledger entries generated by modules"</span>
                            <span class="text-white">"pub struct "</span>
                            <span class="text-[#2ea46f]">"LedgerEntry"</span>
                            <span class="text-white">" {"</span>
                            <div class="pl-4 space-y-1 my-2">
                                <div><span class="text-[#2ea46f]">"pub "</span>"ledger_entry_id: LedgerEntryId,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"company_id: CompanyId,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"user_id: UserId,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"enrollment_id: EnrollmentId,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"source_module: String, "<span class="text-[#527064]">"// e.g. \"RoyalMatrix\""</span></div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"amount: Decimal,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"points: u32,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"status: LedgerStatus, "<span class="text-[#527064]">"// Pending, Approved, Paid, Reversed"</span></div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"reason: String,"</div>
                                <div><span class="text-[#2ea46f]">"pub "</span>"created_at: DateTime&lt;Utc&gt;,"</div>
                            </div>
                            <span class="text-white">"}"</span>
                            <div class="mt-6 pt-4 border-t border-[#1d4837] flex items-center justify-between text-2xs text-[#527064]">
                                <span>"100% Rust Compiled"</span>
                                <span>"PostgreSQL Transactional Safety"</span>
                            </div>
                        </div>
                    </div>
                </div>
            </section>

            // Compensation Mechanics Section
            <section id="mechanics" class="py-20 sm:py-28 bg-paper/50">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="text-center max-w-3xl mx-auto mb-16 sm:mb-20">
                        <span class="text-accent font-extrabold text-sm uppercase tracking-wider">
                            "Built-In Compensation Modules"
                        </span>
                        <h2 class="text-3xl sm:text-5xl font-black text-ink tracking-tight mt-2 mb-4">
                            "Engineered for Rigorous Math and Auditability"
                        </h2>
                        <p class="text-base sm:text-lg text-muted">
                            "Explore the core algorithms built natively into the engine's payplan stack. No arbitrary formulas, no execution gaps. Pure, testable Rust workflows."
                        </p>
                    </div>

                    <div class="grid grid-cols-1 md:grid-cols-2 gap-8">
                        // Flushline
                        <div class="bg-panel-bg border border-border p-8 rounded-2xl shadow-sm hover:shadow-md transition-all flex flex-col justify-between">
                            <div>
                                <div class="w-12 h-12 rounded-xl bg-accent/10 text-accent flex items-center justify-center font-black text-lg mb-6">
                                    "FL"
                                </div>
                                <h3 class="text-xl font-bold text-ink mb-3">"Flushline Linear Graduation"</h3>
                                <p class="text-muted text-sm leading-relaxed mb-6">
                                    "A linear card queue system where accounts advance through five distinct cards (Ten -> Jack -> Queen -> King -> Ace). Graduation occurs after spending 15 cumulative points. Weekly resets target graduated accounts back to King, preserving user-level credentials."
                                </p>
                            </div>
                            <div class="border-t border-paper pt-4 flex justify-between items-center text-xs font-semibold text-muted">
                                <span>"Graduation: 15 Points"</span>
                                <span>"Resets to: King"</span>
                            </div>
                        </div>

                        // Matrix
                        <div class="bg-panel-bg border border-border p-8 rounded-2xl shadow-sm hover:shadow-md transition-all flex flex-col justify-between">
                            <div>
                                <div class="w-12 h-12 rounded-xl bg-accent/10 text-accent flex items-center justify-center font-black text-lg mb-6">
                                    "MX"
                                </div>
                                <h3 class="text-xl font-bold text-ink mb-3">"7-Slot Sponsor-First Matrix"</h3>
                                <p class="text-muted text-sm leading-relaxed mb-6">
                                    "A binary-shaped 7-slot matrix layout (1 owner slot, 6 fill slots). Placements follow a strict sponsor-first order when the sponsor is in the matrix, with a sequential fallback strategy. Cycles generate automatic duplications for continuous flow."
                                </p>
                            </div>
                            <div class="border-t border-paper pt-4 flex justify-between items-center text-xs font-semibold text-muted">
                                <span>"S1 Owner, S2-S7 Fill"</span>
                                <span>"Sponsor Placement Fallback"</span>
                            </div>
                        </div>

                        // Pot Bonus
                        <div class="bg-panel-bg border border-border p-8 rounded-2xl shadow-sm hover:shadow-md transition-all flex flex-col justify-between">
                            <div>
                                <div class="w-12 h-12 rounded-xl bg-accent/10 text-accent flex items-center justify-center font-black text-lg mb-6">
                                    "PT"
                                </div>
                                <h3 class="text-xl font-bold text-ink mb-3">"Dual-Qualification Pot Bonus"</h3>
                                <p class="text-muted text-sm leading-relaxed mb-6">
                                    "Splits the bonus distribution pool into 75% profit sharing (distributed equally to qualified users) and 25% top cycler rewards. User-level qualification strictly requires both: at least one Flushline graduation AND at least one Matrix cycle."
                                </p>
                            </div>
                            <div class="border-t border-paper pt-4 flex justify-between items-center text-xs font-semibold text-muted">
                                <span>"75/25 Pool Split"</span>
                                <span>"FL Grad + Matrix Cycle Req"</span>
                            </div>
                        </div>

                        // Binary
                        <div class="bg-panel-bg border border-border p-8 rounded-2xl shadow-sm hover:shadow-md transition-all flex flex-col justify-between">
                            <div>
                                <div class="w-12 h-12 rounded-xl bg-accent/10 text-accent flex items-center justify-center font-black text-lg mb-6">
                                    "BI"
                                </div>
                                <h3 class="text-xl font-bold text-ink mb-3">"Binary Pairing with Carryover"</h3>
                                <p class="text-muted text-sm leading-relaxed mb-6">
                                    "Organizes enrollments into a binary tree with left and right legs. Volume increments from commissionable points. Periodic pairing jobs matching legs automatically close cycles, updating the Carryover ledger with leftover volume."
                                </p>
                            </div>
                            <div class="border-t border-paper pt-4 flex justify-between items-center text-xs font-semibold text-muted">
                                <span>"Auto-Balance & Legs"</span>
                                <span>"Carryover Retention"</span>
                            </div>
                        </div>
                    </div>
                </div>
            </section>

            // How It Works Section
            <section id="how-it-works" class="py-20 sm:py-28 border-t border-border bg-panel-bg">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="text-center max-w-3xl mx-auto mb-16 sm:mb-20">
                        <span class="text-accent font-extrabold text-sm uppercase tracking-wider">
                            "Process Workflow"
                        </span>
                        <h2 class="text-3xl sm:text-5xl font-black text-ink tracking-tight mt-2 mb-4">
                            "How the Engine Computes Rewards"
                        </h2>
                        <p class="text-base sm:text-lg text-muted">
                            "The platform operates on a reactive, event-driven architecture that tracks every purchase from payment to ledger entry."
                        </p>
                    </div>

                    <div class="relative">
                        // Connection line
                        <div class="hidden md:block absolute left-1/2 top-0 bottom-0 w-0.5 bg-border -translate-x-1/2 z-0"></div>

                        <div class="space-y-16 md:space-y-24 relative z-10">
                            // Step 1
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-8 items-center">
                                <div class="md:text-right md:pr-12">
                                    <div class="inline-flex items-center justify-center w-8 h-8 rounded-full bg-accent text-white font-extrabold text-sm mb-4">
                                        "1"
                                    </div>
                                    <h3 class="text-lg font-bold text-ink mb-2">"Package Purchase"</h3>
                                    <p class="text-muted text-sm max-w-md md:ml-auto">
                                        "A user purchases or subscribes to a package. The system records the transaction, handles either one-time or recurring billing, and updates subscription state."
                                    </p>
                                </div>
                                <div class="hidden md:block md:pl-12">
                                    <div class="w-full bg-paper p-4 rounded-xl border border-border text-xs font-mono text-muted">
                                        "PurchaseCreated { gross_amount: $150, status: Paid }"
                                    </div>
                                </div>
                            </div>

                            // Step 2
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-8 items-center">
                                <div class="hidden md:block md:text-right md:pr-12">
                                    <div class="w-full bg-paper p-4 rounded-xl border border-border text-xs font-mono text-muted">
                                        "EnrollmentCreated { user_id, package_id, sponsor_id }"
                                    </div>
                                </div>
                                <div class="md:pl-12">
                                    <div class="inline-flex items-center justify-center w-8 h-8 rounded-full bg-accent text-white font-extrabold text-sm mb-4">
                                        "2"
                                    </div>
                                    <h3 class="text-lg font-bold text-ink mb-2">"Enrollment & Event Dispatch"</h3>
                                    <p class="text-muted text-sm max-w-md">
                                        "The customer is enrolled in the package. The platform publishes a durable domain event (e.g., PackagePurchased) to the database log."
                                    </p>
                                </div>
                            </div>

                            // Step 3
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-8 items-center">
                                <div class="md:text-right md:pr-12">
                                    <div class="inline-flex items-center justify-center w-8 h-8 rounded-full bg-accent text-white font-extrabold text-sm mb-4">
                                        "3"
                                    </div>
                                    <h3 class="text-lg font-bold text-ink mb-2">"Payplan Stack Runner"</h3>
                                    <p class="text-muted text-sm max-w-md md:ml-auto">
                                        "The engine loads the package's active stack. Built-in modules process the event, executing placement logic, updating cardlines, or tracking volume."
                                    </p>
                                </div>
                                <div class="hidden md:block md:pl-12">
                                    <div class="w-full bg-paper p-4 rounded-xl border border-border text-xs font-mono text-muted">
                                        "StackRunner::run_modules(StackId, Event)"
                                    </div>
                                </div>
                            </div>

                            // Step 4
                            <div class="grid grid-cols-1 md:grid-cols-2 gap-8 items-center">
                                <div class="hidden md:block md:text-right md:pr-12">
                                    <div class="w-full bg-paper p-4 rounded-xl border border-border text-xs font-mono text-muted">
                                        "RewardLedgerEntryCreated { amount, status: Pending }"
                                    </div>
                                </div>
                                <div class="md:pl-12">
                                    <div class="inline-flex items-center justify-center w-8 h-8 rounded-full bg-accent text-white font-extrabold text-sm mb-4">
                                        "4"
                                    </div>
                                    <h3 class="text-lg font-bold text-ink mb-2">"Ledger Entry Creation"</h3>
                                    <p class="text-muted text-sm max-w-md">
                                        "Calculated rewards are saved as ledger entries. Because entries do not directly execute payments, payouts can be audited, confirmed, or adjusted safely."
                                    </p>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </section>

            // Benefits & Reliability Section
            <section id="benefits" class="py-20 sm:py-28 bg-paper/50 border-t border-border">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
                    <div class="grid grid-cols-1 lg:grid-cols-3 gap-8">
                        <div class="bg-panel-bg p-8 rounded-2xl border border-border shadow-sm">
                            <span class="text-xs font-extrabold text-accent tracking-widest uppercase">"Safety"</span>
                            <h3 class="text-xl font-bold text-ink mt-2 mb-4">"Transactional Integrity"</h3>
                            <p class="text-muted text-sm leading-relaxed">
                                "Every workflow—purchases, subscriptions, placements, cycles, and ledger writes—saves inside a single PostgreSQL database transaction. Partial states are strictly impossible."
                            </p>
                        </div>
                        <div class="bg-panel-bg p-8 rounded-2xl border border-border shadow-sm">
                            <span class="text-xs font-extrabold text-accent tracking-widest uppercase">"Auditability"</span>
                            <h3 class="text-xl font-bold text-ink mt-2 mb-4">"Immutable Event Sourcing"</h3>
                            <p class="text-muted text-sm leading-relaxed">
                                "The engine tracks changes using an append-only domain event log. Corrections and resets emit distinct adjustment records, leaving a permanent, readable audit trail."
                            </p>
                        </div>
                        <div class="bg-panel-bg p-8 rounded-2xl border border-border shadow-sm">
                            <span class="text-xs font-extrabold text-accent tracking-widest uppercase">"Decoupled"</span>
                            <h3 class="text-xl font-bold text-ink mt-2 mb-4">"Pure Domain Logic"</h3>
                            <p class="text-muted text-sm leading-relaxed">
                                "Core rules live in a pure library crate completely free from SQL, HTTP, or async runtimes. This ensures calculations are isolated, fast, and 100% testable."
                            </p>
                        </div>
                    </div>
                </div>
            </section>

            // Call to Action
            <section class="py-20 sm:py-28 bg-[#10291f] text-[#d7e9df] relative overflow-hidden">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 text-center relative z-10">
                    <h2 class="text-3xl sm:text-5xl font-black tracking-tight text-white mb-6">
                        "Ready to Configure Your Payplan?"
                    </h2>
                    <p class="text-base sm:text-lg text-[#a9cfb9] max-w-xl mx-auto mb-10">
                        "Log in to the administrative panel to register companies, create catalogs, build packages, and monitor payplan execution logs."
                    </p>
                    <div class="flex flex-col sm:flex-row items-center justify-center gap-4">
                        {if is_authenticated {
                            view! {
                                <a
                                    href="/dashboard"
                                    class="w-full sm:w-auto inline-flex items-center justify-center px-8 py-4 text-base font-bold text-white bg-accent hover:bg-accent/90 rounded-xl transition-all shadow-md"
                                >
                                    "Open Administrative Console"
                                </a>
                            }.into_any()
                        } else {
                            view! {
                                <a
                                    href="/login"
                                    class="w-full sm:w-auto inline-flex items-center justify-center px-8 py-4 text-base font-bold text-white bg-accent hover:bg-accent/90 rounded-xl transition-all shadow-md"
                                >
                                    "Sign In to Administration"
                                </a>
                            }.into_any()
                        }}
                    </div>
                </div>
            </section>

            // Footer
            <footer class="bg-[#10291f] border-t border-[#1d4837] py-12 text-sm text-[#527064]">
                <div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 flex flex-col md:flex-row items-center justify-between gap-6">
                    <div class="flex items-center gap-3">
                        <span class="flex items-center justify-center w-8 h-8 rounded-lg bg-gradient-to-tr from-accent to-accent/80 text-white font-extrabold text-base">
                            "P"
                        </span>
                        <span class="text-white font-bold tracking-tight">"PayPlan Platform"</span>
                    </div>
                    <div class="text-[#a9cfb9]/60 text-xs">
                        "© 2026 PayPlan Platform. All rights reserved."
                    </div>
                </div>
            </footer>
        </div>
    }
}
