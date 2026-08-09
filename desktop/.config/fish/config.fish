if test -r /usr/share/cachyos-fish-config/cachyos-config.fish
    source /usr/share/cachyos-fish-config/cachyos-config.fish
end

# User-installed and dotfiles-managed commands live here. Fish does not read
# ~/.bashrc, so add the directory explicitly instead of relying on inheritance.
fish_add_path --move --path "$HOME/.local/bin"

# Open directly at the prompt instead of showing the CachyOS system summary.
function fish_greeting
end

# Keep the prompt compact and monochrome.
set -g pure_enable_single_line_prompt false
set -g pure_symbol_prompt '$'
set -g pure_color_current_directory normal
set -g pure_color_prompt_on_success normal
set -g pure_color_prompt_on_error normal

# Muted interactive syntax colors for a low-contrast terminal.
set -g fish_color_normal d8d8d8
set -g fish_color_command b8b8b8
set -g fish_color_keyword b8b8b8
set -g fish_color_param a8a8a8
set -g fish_color_option 9aa0a6
set -g fish_color_valid_path a8a8a8 --underline
set -g fish_color_operator 8f9aa8
set -g fish_color_redirection 8f9aa8
set -g fish_color_escape 8f9aa8
set -g fish_color_quote a8a8a8
set -g fish_color_comment 666666
set -g fish_color_error a87878
set -g fish_color_autosuggestion 555555
set -g fish_color_selection d8d8d8 --background=333333
set -g fish_color_search_match d8d8d8 --background=333333

# Subtle eza/ls palette: slate directories with neutral metadata.
set -gx EZA_COLORS 'di=38;5;67:fi=38;5;250:ex=38;5;109:ln=38;5;109:or=38;5;95:pi=38;5;109:so=38;5;109:bd=38;5;245:cd=38;5;245:sp=38;5;245:mp=38;5;67:ic=38;5;67:ur=38;5;245:uw=38;5;245:ux=38;5;245:ue=38;5;245:gr=38;5;242:gw=38;5;242:gx=38;5;242:tr=38;5;242:tw=38;5;242:tx=38;5;242:su=38;5;95:sf=38;5;95:uu=38;5;250:un=38;5;245:gu=38;5;245:gn=38;5;242:sn=38;5;245:sb=38;5;242:da=38;5;242:xx=38;5;240:lc=38;5;242:lm=38;5;245:hd=38;5;245:lp=38;5;109:ga=38;5;109:gm=38;5;137:gd=38;5;95:gv=38;5;109:gt=38;5;137:gi=38;5;240:gc=38;5;95'
