use crate::api::{AssetInfo, TransactionInfo, TransactionStatus, TransactionType, TreasuryInfo, TreasuryService};
use crate::components::Modal;
use leptos::task::spawn_local;
use crate::state::use_app_state;
use leptos::prelude::*;
use leptos::view;
use leptos_meta::*;
use leptos_router::components::A;

/// Format a number with thousand separators
fn format_with_commas(num: impl ToString, suffix: &str) -> String {
    let num_str = num.to_string();
    let parts: Vec<&str> = num_str.split('.').collect();
    let int_part = parts[0];
    let frac_part = if parts.len() > 1 {
        Some(parts[1])
    } else {
        None
    };

    let mut result = String::new();
    let mut count = 0;

    for c in int_part.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    let mut formatted = result.chars().rev().collect::<String>();
    if let Some(frac) = frac_part {
        formatted.push('.');
        formatted.push_str(frac);
    }

    if !suffix.is_empty() {
        formatted.push(' ');
        formatted.push_str(suffix);
    }

    formatted
}

/// Format a float with thousand separators and 2 decimal places
fn format_usd(value: f64) -> String {
    if value <= 0.0 {
        return "-".to_string();
    }
    let formatted = format!("{:.2}", value);
    let parts: Vec<&str> = formatted.split('.').collect();
    let int_part = parts[0];
    let frac_part = parts[1];

    let mut result = String::new();
    let mut count = 0;

    for c in int_part.chars().rev() {
        if count > 0 && count % 3 == 0 {
            result.push(',');
        }
        result.push(c);
        count += 1;
    }

    let mut formatted = result.chars().rev().collect::<String>();
    formatted.push('.');
    formatted.push_str(frac_part);

    formatted
}

#[component]
pub fn TreasuryPage() -> impl IntoView {
    let app_state = use_app_state();

    // Fetch treasury data - use LocalResource for CSR
    let treasury = LocalResource::new(move || {
        let service = app_state.treasury_service();
        let loading = app_state.loading();
        async move {
            loading.treasury.set(true);
            let result = service.get_info().await;
            loading.treasury.set(false);
            result
        }
    });

    // Transfer modal state
    let transfer_open = RwSignal::new(false);
    let transfer_to = RwSignal::new(String::new());
    let transfer_amount = RwSignal::new(String::new());
    let transfer_saving = RwSignal::new(false);
    let transfer_error = RwSignal::new(None::<String>);
    let transfer_success = RwSignal::new(None::<String>);

    let client = crate::api::create_client();
    let treasury_service = TreasuryService::new(client);
    let service_stored = StoredValue::new(treasury_service);

    let on_transfer = move || {
        let to = transfer_to.get();
        let amount = transfer_amount.get();
        if to.is_empty() || amount.is_empty() {
            transfer_error.set(Some("Address and amount are required".to_string()));
            return;
        }
        transfer_saving.set(true);
        transfer_error.set(None);
        transfer_success.set(None);
        let service = service_stored.get_value();
        spawn_local(async move {
            match service.transfer(&to, &amount).await {
                Ok(resp) => {
                    transfer_saving.set(false);
                    transfer_success.set(Some(format!("Transfer submitted: {}", resp.get("tx_hash").and_then(|v| v.as_str()).unwrap_or("N/A"))));
                    transfer_to.set(String::new());
                    transfer_amount.set(String::new());
                }
                Err(e) => {
                    transfer_saving.set(false);
                    transfer_error.set(Some(format!("Transfer failed: {}", e)));
                }
            }
        });
    };

    view! {
        <Title text="Treasury - BeeBotOS" />
        <div class="page treasury-page">
            <div class="page-header">
                <div class="breadcrumb-nav">
                    <A href="/dao">"DAO"</A>
                    <span>"/"</span>
                    <span>"Treasury"</span>
                </div>
                <h1>"DAO Treasury"</h1>
                <p class="page-description">"Manage community funds with transparent, on-chain governance"</p>
            </div>

            <Suspense fallback=|| view! { <TreasuryLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match treasury.await {
                            Ok(data) => view! { <TreasuryView data=data on_transfer=move || transfer_open.set(true)/> }.into_any(),
                            Err(e) => view! { <TreasuryError message=e.to_string()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>

            // Transfer Modal
            {move || if transfer_open.get() {
                view! {
                    <Modal title="Transfer" on_close=move || transfer_open.set(false)>
                        <div class="modal-body">
                            {move || transfer_error.get().map(|msg| view! {
                                <div class="alert alert-error">{msg}</div>
                            })}
                            {move || transfer_success.get().map(|msg| view! {
                                <div class="alert alert-success">{msg}</div>
                            })}
                            <div class="form-group">
                                <label>"To Address"</label>
                                <input
                                    type="text"
                                    prop:value=transfer_to
                                    on:input=move |e| transfer_to.set(event_target_value(&e))
                                    placeholder="0x..."
                                />
                            </div>
                            <div class="form-group">
                                <label>"Amount (wei)"</label>
                                <input
                                    type="text"
                                    prop:value=transfer_amount
                                    on:input=move |e| transfer_amount.set(event_target_value(&e))
                                    placeholder="1000000000000000000"
                                />
                            </div>
                        </div>
                        <div class="modal-footer">
                            <button class="btn btn-secondary" on:click=move |_| transfer_open.set(false)>"Cancel"</button>
                            <button
                                class="btn btn-primary"
                                on:click={
                                    let on_transfer = on_transfer.clone();
                                    move |_| on_transfer()
                                }
                                disabled=transfer_saving
                            >
                                {move || if transfer_saving.get() { "Submitting..." } else { "Submit Transfer" }}
                            </button>
                        </div>
                    </Modal>
                }.into_any()
            } else {
                ().into_any()
            }}
        </div>
    }
}

#[component]
fn TreasuryView(data: TreasuryInfo, on_transfer: impl Fn() + Clone + 'static) -> impl IntoView {
    let on_transfer = std::rc::Rc::new(std::cell::RefCell::new(on_transfer));
    view! {
        <div class="treasury-content">
            <section class="treasury-overview">
                <div class="total-balance-card">
                    <div class="balance-header">
                        <span class="balance-label">"Total Treasury Balance"</span>
                        <span class="live-indicator">"● Live"</span>
                    </div>
                    <div class="balance-value">
                        {format_with_commas(data.total_balance, &data.token_symbol)}
                    </div>
                    <div class="balance-actions">
                        <button class="btn btn-primary">"Deposit"</button>
                        <button class="btn btn-secondary">"Withdraw"</button>
                        <button class="btn btn-secondary" on:click=move |_| on_transfer.borrow_mut()()>"Transfer"</button>
                    </div>
                </div>
            </section>

            <div class="treasury-grid">
                <section class="card assets-section">
                    <div class="section-header">
                        <h2>"Assets"</h2>
                        <span class="asset-count">{format!("{} tokens", data.assets.len())}</span>
                    </div>

                    {move || if data.assets.is_empty() {
                        view! { <AssetsEmpty/> }.into_any()
                    } else {
                        view! {
                            <div class="assets-list">
                                {data.assets.clone().into_iter().map(|asset| view! {
                                    <AssetRow asset=asset/>
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </section>

                <section class="card transactions-section">
                    <div class="section-header">
                        <h2>"Recent Transactions"</h2>
                        <A href="/dao/treasury/transactions" attr:class="btn btn-text">
                            "View All →"
                        </A>
                    </div>

                    {move || if data.recent_transactions.is_empty() {
                        view! { <TransactionsEmpty/> }.into_any()
                    } else {
                        view! {
                            <div class="transactions-list">
                                {data.recent_transactions.clone().into_iter().map(|tx| view! {
                                    <TransactionRow tx=tx/>
                                }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </section>
            </div>

            <section class="card treasury-info">
                <h3>"About the Treasury"</h3>
                <div class="info-grid">
                    <div class="info-item">
                        <span class="info-icon">"🔒"</span>
                        <div>
                            <h4>"Multi-Sig Protected"</h4>
                            <p>"All withdrawals require multiple signatures from DAO council members"</p>
                        </div>
                    </div>
                    <div class="info-item">
                        <span class="info-icon">"📊"</span>
                        <div>
                            <h4>"Transparent"</h4>
                            <p>"All transactions are recorded on-chain and publicly verifiable"</p>
                        </div>
                    </div>
                    <div class="info-item">
                        <span class="info-icon">"⚡"</span>
                        <div>
                            <h4>"Governance Controlled"</h4>
                            <p>"Major allocations require community vote through DAO proposals"</p>
                        </div>
                    </div>
                </div>
            </section>
        </div>
    }
}

#[component]
fn AssetRow(#[prop(into)] asset: AssetInfo) -> impl IntoView {
    view! {
        <div class="asset-row">
            <div class="asset-info">
                <div class="asset-token">{asset.token.clone()}</div>
                <div class="asset-balance">{format_with_commas(&asset.balance, "")}</div>
            </div>
            <div class="asset-value">
                {if asset.value_usd > 0.0 {
                    format!("${}", format_usd(asset.value_usd))
                } else {
                    "-".to_string()
                }}
            </div>
        </div>
    }
}

#[component]
fn TransactionRow(#[prop(into)] tx: TransactionInfo) -> impl IntoView {
    let status_class = match tx.status {
        TransactionStatus::Completed => "status-completed",
        TransactionStatus::Pending => "status-pending",
        TransactionStatus::Failed => "status-failed",
    };

    let type_icon = match tx.tx_type {
        TransactionType::Deposit => "⬇️",
        TransactionType::Withdrawal => "⬆️",
        TransactionType::Transfer => "↔️",
        TransactionType::Swap => "🔄",
    };

    view! {
        <div class="transaction-row">
            <div class="transaction-icon">{type_icon}</div>
            <div class="transaction-details">
                <div class="transaction-type">{format!("{:?}", tx.tx_type)}</div>
                <div class="transaction-meta">
                    <span class="transaction-time">{tx.timestamp}</span>
                    <span class=format!("transaction-status {}", status_class)>
                        {format!("{:?}", tx.status)}
                    </span>
                </div>
            </div>
            <div class="transaction-amount">
                {format!("{:+} {}", tx.amount, tx.token)}
            </div>
        </div>
    }
}

#[component]
fn AssetsEmpty() -> impl IntoView {
    view! {
        <div class="empty-state-small">
            <p class="text-muted">"No assets in treasury"</p>
            <button class="btn btn-primary btn-sm">"Make First Deposit"</button>
        </div>
    }
}

#[component]
fn TransactionsEmpty() -> impl IntoView {
    view! {
        <div class="empty-state-small">
            <p class="text-muted">"No recent transactions"</p>
        </div>
    }
}

#[component]
fn TreasuryLoading() -> impl IntoView {
    view! {
        <div class="treasury-skeleton">
            <div class="total-balance-card skeleton">
                <div class="skeleton-label"></div>
                <div class="skeleton-value"></div>
            </div>
            <div class="treasury-grid">
                <div class="card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
                <div class="card skeleton">
                    <div class="skeleton-header"></div>
                    <div class="skeleton-line"></div>
                    <div class="skeleton-line"></div>
                </div>
            </div>
        </div>
    }
}

#[component]
fn TreasuryError(#[prop(into)] message: String) -> impl IntoView {
    view! {
        <div class="error-state">
            <div class="error-icon">"⚠️"</div>
            <h3>"Failed to load treasury"</h3>
            <p>{message}</p>
            <button
                class="btn btn-primary"
                on:click=move |_| { let _ = window().location().reload(); }
            >
                "Retry"
            </button>
        </div>
    }
}

/// Full transactions history page
#[component]
pub fn TreasuryTransactionsPage() -> impl IntoView {
    let app_state = use_app_state();

    let treasury = LocalResource::new(move || {
        let service = app_state.treasury_service();
        async move { service.get_info().await }
    });

    view! {
        <Title text="Treasury Transactions - BeeBotOS" />
        <div class="page treasury-page">
            <div class="page-header">
                <div class="breadcrumb-nav">
                    <A href="/dao">"DAO"</A>
                    <span>"/"</span>
                    <A href="/dao/treasury">"Treasury"</A>
                    <span>"/"</span>
                    <span>"Transactions"</span>
                </div>
                <h1>"Transaction History"</h1>
                <p class="page-description">"All treasury transactions are recorded on-chain"</p>
            </div>

            <Suspense fallback=|| view! { <TreasuryLoading/> }>
                {move || {
                    Suspend::new(async move {
                        match treasury.await {
                            Ok(data) => view! {
                                <section class="card transactions-section">
                                    <div class="section-header">
                                        <h2>"All Transactions"</h2>
                                        <span class="transaction-count">{format!("{} total", data.recent_transactions.len())}</span>
                                    </div>
                                    {if data.recent_transactions.is_empty() {
                                        view! { <TransactionsEmpty/> }.into_any()
                                    } else {
                                        view! {
                                            <div class="transactions-list">
                                                {data.recent_transactions.into_iter().map(|tx| view! {
                                                    <TransactionRow tx=tx/>
                                                }).collect::<Vec<_>>()}
                                            </div>
                                        }.into_any()
                                    }}
                                </section>
                            }.into_any(),
                            Err(e) => view! { <TreasuryError message=e.to_string()/> }.into_any(),
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}
