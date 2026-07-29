//! Banner ASCII exibido na abertura do CLI — nome de exibição do projeto.

pub const BANNER: &str = r#"
   _____ ____ ______  ______  __  __ ____  _______  __
  / ___// __ \/ ____/ / __ \ \/ /_/ // __ \/_  __/ / / /
  \__ \/ /_/ / /     / /_/ /\  __// /_/ / / / / /_/ /
 ___/ / ____/ /___  / ____/ / /  / _, _/ / / / __  /
/____/_/    \____/ /_/     /_/  /_/ |_| /_/ /_/ /_/

  ____  ____   ____ ______ ______ ____ _______ _____ ____  __
 / __ \/ __ \ / __ \/_  __// ____// __ \_  __ // ___// __ \/ /
/ /_/ / /_/ // / / / / /  / __/  / / / // /_/ / \__ \/ / / / /
\ ,__/\__, // /_/ / / /  / /___ / /_/ // ____/ ___/ / /_/ /_/
/_/     /_/ \____/ /_/  /_____/ \____//_/    /____/\____(_)

 ____   ____ ______ _   __ ______ __
/ __ ) / __ \/_  __// | / // ____// /
/ __  |/ / / / / /  /  |/ // __/  / /
/ /_/ // /_/ / / /  / /|  // /___ / /___
/_____/ \____/ /_/  /_/ |_//_____//_____/

   [ SECURITY · PROTECTIVE · BOTNET · DEFENSIVE ]
   -----------------------------------------------
        volume anomaly detection · shared blacklist
"#;

pub fn print_banner() {
    // silencia o banner em modo não-interativo, ex: pipeline/systemd com stdout redirecionado
    if atty_stdout() {
        println!("{}", BANNER);
    }
}

fn atty_stdout() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal()
}
