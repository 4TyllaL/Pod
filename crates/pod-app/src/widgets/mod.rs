//! Registro dos widgets internos. Adicionar um widget = um módulo + uma entrada aqui; o host,
//! o storage e o renderer não mudam.

pub mod agenda;
pub mod calendar;
pub mod claude_usage;
pub mod clock;
pub mod converter;
pub mod countdown;
pub mod clipboard;
pub mod favorites;
pub mod files;
pub mod gauge;
pub mod music;
pub mod github;
pub mod launcher;
pub mod habits;
pub mod holidays;
pub mod network;
pub mod stocks;
pub mod news;
pub mod notifications;
pub mod overview;
pub mod parcels;
pub mod pomodoro;
pub mod quick_notes;
pub mod reminders;
pub mod stopwatch;
pub mod system;
pub mod test_card;
pub mod timer;
pub mod water;
pub mod weather;
pub mod worldclock;
pub mod tkit;
pub mod time_progress;

use crate::runtime::Runtime;

pub fn register_all(rt: &mut Runtime) {
    rt.register(quick_notes::descriptor(), quick_notes::create);
    rt.register(test_card::descriptor(), test_card::create);
    rt.register(clock::descriptor(), clock::create);
    rt.register(time_progress::descriptor(), time_progress::create);
    rt.register(water::descriptor(), water::create);
    rt.register(converter::descriptor(), converter::create);
    rt.register(favorites::descriptor(), favorites::create);
    rt.register(parcels::descriptor(), parcels::create);
    rt.register(countdown::descriptor(), countdown::create);
    rt.register(calendar::descriptor(), calendar::create);
    rt.register(reminders::descriptor(), reminders::create);
    rt.register(timer::descriptor(), timer::create);
    rt.register(stopwatch::descriptor(), stopwatch::create);
    rt.register(pomodoro::descriptor(), pomodoro::create);
    rt.register(system::descriptor(system::Kind::Cpu), system::create_cpu);
    rt.register(system::descriptor(system::Kind::Memory), system::create_memory);
    rt.register(system::descriptor(system::Kind::Disk), system::create_disk);
    rt.register(system::descriptor(system::Kind::Battery), system::create_battery);
    rt.register(overview::descriptor(), overview::create);
    rt.register(network::descriptor(), network::create);
    rt.register(music::descriptor(), music::create);
    rt.register(notifications::descriptor(), notifications::create);
    rt.register(claude_usage::descriptor(), claude_usage::create);
    rt.register(weather::descriptor(), weather::create);
    rt.register(news::descriptor(), news::create);
    rt.register(github::descriptor(), github::create);
    rt.register(habits::descriptor(), habits::create);
    rt.register(holidays::descriptor(), holidays::create);
    rt.register(stocks::descriptor(), stocks::create);
    rt.register(agenda::descriptor(), agenda::create);
    rt.register(launcher::descriptor(), launcher::create);
    rt.register(worldclock::descriptor(), worldclock::create);
    rt.register(files::descriptor(files::Kind::Downloads), files::create_downloads);
    rt.register(files::descriptor(files::Kind::Screenshots), files::create_screenshots);
    rt.register(clipboard::descriptor(), clipboard::create);
}

/// Widgets de cartão que o usuário pode criar pela bandeja: (tipo, nome em pt-BR).
pub fn catalog() -> Vec<(pod_model::ids::WidgetTypeId, &'static str)> {
    vec![
        (clock::TYPE_ID, "Rel\u{f3}gio"),
        (time_progress::TYPE_ID, "Progresso do tempo"),
        (calendar::TYPE_ID, "Calend\u{e1}rio"),
        (countdown::TYPE_ID, "Contagem regressiva"),
        (reminders::TYPE_ID, "Lembretes"),
        (water::TYPE_ID, "\u{c1}gua"),
        (converter::TYPE_ID, "Conversor"),
        (favorites::TYPE_ID, "Favoritos"),
        (parcels::TYPE_ID, "Encomendas"),
        (timer::TYPE_ID, "Timer"),
        (stopwatch::TYPE_ID, "Cron\u{f4}metro"),
        (pomodoro::TYPE_ID, "Pomodoro"),
        (system::CPU, "Processador"),
        (system::MEMORY, "Mem\u{f3}ria"),
        (system::DISK, "Disco"),
        (system::BATTERY, "Bateria"),
        (overview::TYPE_ID, "Sistema"),
        (network::TYPE_ID, "Rede"),
        (music::TYPE_ID, "Música"),
        (notifications::TYPE_ID, "Notificações"),
        (claude_usage::TYPE_ID, "Uso do Claude"),
        (weather::TYPE_ID, "Clima (internet)"),
        (news::TYPE_ID, "Notícias (internet)"),
        (github::TYPE_ID, "GitHub (internet)"),
        (habits::TYPE_ID, "Hábitos"),
        (holidays::TYPE_ID, "Feriados"),
        (stocks::TYPE_ID, "Bolsa (internet)"),
        (agenda::TYPE_ID, "Agenda"),
        (launcher::TYPE_ID, "Atalhos"),
        (worldclock::TYPE_ID, "Hora no mundo"),
        (files::DOWNLOADS, "Downloads"),
        (files::SCREENSHOTS, "Capturas de tela"),
        (clipboard::TYPE_ID, "\u{c1}rea de transfer\u{ea}ncia"),
    ]
}

/// Categorias da galeria "Adicionar widget" (a primeira, "Todos", não filtra).
pub const CATEGORIES: [&str; 6] = ["Todos", "Tempo", "Produtividade", "Sistema", "Arquivos", "Internet"];

/// Índice em `CATEGORIES` (1..) do tipo; 0 se não tem categoria.
/// Tipos com layout próprio para 3 ou 4 colunas (os demais esticam o de 2 colunas).
pub fn wide_aware(t: &pod_model::ids::WidgetTypeId) -> bool {
    matches!(t.0, "pod.weather" | "pod.news" | "pod.calendar" | "pod.worldclock" | "pod.stocks")
}

pub fn category_of(type_id: &pod_model::ids::WidgetTypeId) -> usize {
    match type_id.0 {
        "pod.clock" | "pod.holidays" | "pod.worldclock" | "pod.agenda" | "pod.time-progress" | "pod.calendar" | "pod.countdown" | "pod.timer" | "pod.stopwatch" | "pod.pomodoro" => 1,
        "pod.reminders" | "pod.launcher" | "pod.habits" | "pod.water" | "pod.converter" | "pod.favorites" | "pod.parcels" | "pod.music" => 2,
        "pod.cpu" | "pod.memory" | "pod.disk" | "pod.battery" | "pod.overview" | "pod.network" | "pod.claude-usage" => 3,
        "pod.weather" | "pod.news" | "pod.github" | "pod.stocks" => 5,
        "pod.downloads" | "pod.screenshots" | "pod.clipboard" | "pod.notifications" => 4,
        _ => 0,
    }
}

/// Cor de identidade do cartão (índice em `Theme::accents`: 0 azul, 1 vermelho-alaranjado, 2 verde,
/// 3 roxo). Cada tipo tem a sua, como nos widgets de referência; `None` = sem preferência.
pub fn accent_for(type_id: &pod_model::ids::WidgetTypeId) -> Option<usize> {
    let a = match type_id.0 {
        "pod.clock" | "pod.worldclock" | "pod.reminders" | "pod.water" | "pod.network" | "pod.clipboard" | "pod.cpu" | "pod.notifications" | "pod.weather" => 0,
        "pod.calendar" | "pod.countdown" | "pod.timer" | "pod.pomodoro" | "pod.music" | "pod.claude-usage" | "pod.news" | "pod.github" | "pod.stocks" => 1,
        "pod.disk" | "pod.battery" | "pod.downloads" | "pod.parcels" | "pod.habits" | "pod.holidays" => 2,
        "pod.time-progress" | "pod.launcher" | "pod.converter" | "pod.favorites" | "pod.stopwatch" | "pod.memory" | "pod.screenshots" => 3,
        _ => return None,
    };
    Some(a)
}

/// Cartões da primeira execução (quando ainda não existe workspace gravado): só os que não usam a internet nem leem
/// nada fora do Pod, para a área de trabalho não nascer vazia.
pub fn first_run_defaults() -> [pod_model::ids::WidgetTypeId; 3] {
    [clock::TYPE_ID, calendar::TYPE_ID, overview::TYPE_ID]
}

/// Nome de exibição de um tipo (bandeja, acessibilidade).
pub fn title_of(type_id: &pod_model::ids::WidgetTypeId) -> Option<&'static str> {
    catalog().into_iter().find(|(t, _)| t == type_id).map(|(_, n)| n)
}

#[cfg(test)]
mod category_tests {
    use super::*;

    #[test]
    fn every_catalog_widget_has_a_real_category() {
        for (t, name) in catalog() {
            let c = category_of(&t);
            assert!((1..CATEGORIES.len()).contains(&c), "{name} ({}) sem categoria", t.0);
        }
        // Nenhuma categoria fica vazia.
        for c in 1..CATEGORIES.len() {
            assert!(catalog().iter().any(|(t, _)| category_of(t) == c), "{} vazia", CATEGORIES[c]);
        }
    }
    #[test]
    fn first_run_defaults_are_catalog_widgets_that_do_not_use_the_internet() {
        let defaults = first_run_defaults();
        for t in &defaults {
            assert!(catalog().iter().any(|(c, _)| c == t), "{} fora do catálogo", t.0);
            assert_ne!(CATEGORIES[category_of(t)], "Internet", "{} usa a internet", t.0);
            assert!(descriptor_for_test(t), "{} sem formato padrão", t.0);
        }
        let mut names: Vec<&str> = defaults.iter().map(|t| t.0).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), defaults.len(), "sem repetição");
    }

    /// Todos os padrões são cartões de grade com um formato padrão.
    fn descriptor_for_test(t: &pod_model::ids::WidgetTypeId) -> bool {
        match t.0 {
            "pod.clock" => clock::descriptor().default_shape.is_some(),
            "pod.calendar" => calendar::descriptor().default_shape.is_some(),
            "pod.overview" => overview::descriptor().default_shape.is_some(),
            _ => false,
        }
    }
}
