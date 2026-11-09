use leptos::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControlPage {
    Interaction,
    Settings,
    Memory,
    Files,
    Management,
    Updates,
}

impl ControlPage {
    const ALL: [Self; 6] = [
        Self::Interaction,
        Self::Settings,
        Self::Memory,
        Self::Files,
        Self::Management,
        Self::Updates,
    ];

    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::Interaction => "pet-interaction",
            Self::Settings => "pet-settings",
            Self::Memory => "pet-memory",
            Self::Files => "pet-files",
            Self::Management => "pet-management",
            Self::Updates => "pet-updates",
        }
    }

    fn tab_id(self) -> String {
        format!("{}-tab", self.id())
    }

    pub(crate) fn focus_tab(self) {
        use wasm_bindgen::JsCast;
        if let Some(tab) = document()
            .get_element_by_id(&self.tab_id())
            .and_then(|node| node.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = tab.focus();
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Interaction => "互動",
            Self::Settings => "設定",
            Self::Memory => "記憶",
            Self::Files => "檔案",
            Self::Management => "管理",
            Self::Updates => "更新",
        }
    }

    fn from_key(index: usize, key: &str) -> Option<Self> {
        let last = Self::ALL.len() - 1;
        let next = match key {
            "ArrowLeft" | "ArrowUp" => {
                if index == 0 {
                    last
                } else {
                    index - 1
                }
            }
            "ArrowRight" | "ArrowDown" => {
                if index == last {
                    0
                } else {
                    index + 1
                }
            }
            "Home" => 0,
            "End" => last,
            _ => return None,
        };
        Some(Self::ALL[next])
    }
}

#[component]
pub(crate) fn ControlNavigation(page: RwSignal<ControlPage>) -> impl IntoView {
    view! {
         <div class="control-navigation" role="tablist" aria-label="寵物控制台功能區">
            {ControlPage::ALL.into_iter().enumerate().map(|(index, item)| view! {
                <button
                    id=item.tab_id()
                    type="button"
                    role="tab"
                    aria-controls=item.id()
                    aria-selected=move || (page.get() == item).to_string()
                    tabindex=move || if page.get() == item { "0" } else { "-1" }
                    on:click=move |_| page.set(item)
                    on:keydown=move |event: leptos::ev::KeyboardEvent| {
                        if let Some(next) = ControlPage::from_key(index, &event.key()) {
                            event.prevent_default();
                            page.set(next);
                            next.focus_tab();
                        }
                    }
                >{item.label()}</button>
            }).collect_view()}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_exposes_one_selected_tab_and_six_panels() {
        Owner::new().with(|| {
            let page = RwSignal::new(ControlPage::Interaction);
            let html = view! { <ControlNavigation page /> }.to_html();
            for item in ControlPage::ALL {
                assert!(html.contains(&format!("aria-controls=\"{}\"", item.id())));
                assert!(html.contains(item.label()));
            }
            assert_eq!(html.matches("role=\"tab\"").count(), 6);
            assert_eq!(html.matches("aria-selected=\"true\"").count(), 1);
            assert_eq!(html.matches("tabindex=\"0\"").count(), 1);
            assert_eq!(html.matches("tabindex=\"-1\"").count(), 5);
        });
    }

    #[test]
    fn arrow_keys_wrap_and_home_and_end_select_endpoints() {
        assert_eq!(
            ControlPage::from_key(0, "ArrowUp").unwrap().id(),
            "pet-updates"
        );
        assert_eq!(
            ControlPage::from_key(5, "ArrowRight").unwrap().id(),
            "pet-interaction"
        );
        assert_eq!(
            ControlPage::from_key(3, "Home").unwrap().id(),
            "pet-interaction"
        );
        assert_eq!(ControlPage::from_key(3, "End").unwrap().id(), "pet-updates");
        assert!(ControlPage::from_key(0, "Tab").is_none());
    }
}
