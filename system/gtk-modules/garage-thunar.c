#include <gtk/gtk.h>
#include <thunarx/thunarx.h>

#define GARAGE_ITEM_HOVER_CLASS "garage-shortcut-item-hover"

typedef struct
{
  GtkWidget *toolbar;
  GtkWidget *sidepane;
  GtkWidget *content_shell;
} GarageWindowChrome;

static gboolean
garage_is_shortcuts_view (GtkWidget *widget)
{
  GtkTreeModel *model;

  if (!GTK_IS_TREE_VIEW (widget))
    return FALSE;
  model = gtk_tree_view_get_model (GTK_TREE_VIEW (widget));
  return model != NULL
         && gtk_tree_model_get_n_columns (model) >= 2
         && gtk_tree_model_get_column_type (model, 0) == G_TYPE_BOOLEAN
         && gtk_tree_model_get_column_type (model, 1) == G_TYPE_BOOLEAN;
}

static gboolean
garage_is_standard_view (GtkWidget *widget)
{
  GtkWidget *ancestor;

  for (ancestor = widget; ancestor != NULL;
       ancestor = gtk_widget_get_parent (ancestor))
    if (gtk_style_context_has_class (gtk_widget_get_style_context (ancestor),
                                     "standard-view"))
      return TRUE;
  return FALSE;
}

static void
garage_set_widget (GtkWidget **slot, GtkWidget *widget)
{
  if (*slot == widget)
    return;
  if (*slot != NULL)
    g_object_remove_weak_pointer (G_OBJECT (*slot), (gpointer *) slot);
  *slot = widget;
  if (widget != NULL)
    g_object_add_weak_pointer (G_OBJECT (widget), (gpointer *) slot);
}

static void
garage_window_chrome_free (gpointer data)
{
  GarageWindowChrome *chrome = data;

  garage_set_widget (&chrome->toolbar, NULL);
  garage_set_widget (&chrome->sidepane, NULL);
  garage_set_widget (&chrome->content_shell, NULL);
  g_free (chrome);
}

static gboolean
garage_shortcuts_motion (GtkWidget      *widget,
                         GdkEventMotion *event,
                         gpointer        data)
{
  GtkTreeModel *model;
  GtkTreePath *path = NULL;
  GtkTreeIter iter;
  gboolean is_header = TRUE;
  gboolean is_item = FALSE;
  GtkStyleContext *context = gtk_widget_get_style_context (widget);

  (void) data;
  if (gtk_tree_view_get_path_at_pos (GTK_TREE_VIEW (widget),
                                     (gint) event->x, (gint) event->y,
                                     &path, NULL, NULL, NULL))
    {
      model = gtk_tree_view_get_model (GTK_TREE_VIEW (widget));
      if (gtk_tree_model_get_iter (model, &iter, path))
        {
          gtk_tree_model_get (model, &iter, 0, &is_header, -1);
          is_item = !is_header;
        }
      gtk_tree_path_free (path);
    }

  if (is_item)
    gtk_style_context_add_class (context, GARAGE_ITEM_HOVER_CLASS);
  else
    gtk_style_context_remove_class (context, GARAGE_ITEM_HOVER_CLASS);
  gtk_widget_queue_draw (widget);
  return GDK_EVENT_PROPAGATE;
}

static gboolean
garage_shortcuts_leave (GtkWidget        *widget,
                        GdkEventCrossing *event,
                        gpointer          data)
{
  (void) event;
  (void) data;
  gtk_style_context_remove_class (gtk_widget_get_style_context (widget),
                                  GARAGE_ITEM_HOVER_CLASS);
  gtk_widget_queue_draw (widget);
  return GDK_EVENT_PROPAGATE;
}

static gboolean
garage_details_draw_stripes (GtkWidget *widget, cairo_t *cr, gpointer data)
{
  GtkTreeView *tree = GTK_TREE_VIEW (widget);
  GtkTreeModel *model = gtk_tree_view_get_model (tree);
  GtkTreePath *first_path;
  GdkRectangle row;
  GtkAllocation allocation;
  GtkStyleContext *context;
  GtkTreeSelection *selection;
  GdkRGBA foreground;
  gint content_x = 0;
  gint content_y = 0;
  gint first_y = 0;
  gint stripe;

  (void) data;
  if (model == NULL || !gtk_tree_model_iter_n_children (model, NULL))
    return GDK_EVENT_PROPAGATE;

  first_path = gtk_tree_path_new_first ();
  gtk_tree_view_get_background_area (tree, first_path, NULL, &row);
  gtk_tree_path_free (first_path);
  if (row.height <= 0)
    return GDK_EVENT_PROPAGATE;

  gtk_tree_view_convert_bin_window_to_widget_coords (tree, 0, 0,
                                                      &content_x, &content_y);
  gtk_tree_view_convert_tree_to_widget_coords (tree, 0, row.y,
                                                NULL, &first_y);
  gtk_widget_get_allocation (widget, &allocation);
  context = gtk_widget_get_style_context (widget);
  selection = gtk_tree_view_get_selection (tree);
  if (!gtk_style_context_lookup_color (context, "view_fg_color", &foreground))
    foreground = (GdkRGBA) { 1.0, 1.0, 1.0, 1.0 };

  stripe = first_y < content_y ? (content_y - first_y) / row.height : 0;
  cairo_save (cr);
  cairo_rectangle (cr, content_x, content_y,
                   allocation.width - content_x,
                   allocation.height - content_y);
  cairo_clip (cr);
  cairo_set_source_rgba (cr, foreground.red, foreground.green,
                         foreground.blue, 0.035);
  for (; first_y + stripe * row.height < allocation.height; stripe++)
    if ((stripe & 1) != 0)
      {
        GtkTreePath *path = NULL;
        gint bin_x = 0;
        gint bin_y = 0;

        gtk_tree_view_convert_widget_to_bin_window_coords (
          tree, content_x, first_y + stripe * row.height + row.height / 2,
          &bin_x, &bin_y);
        if (gtk_tree_view_get_path_at_pos (tree, bin_x, bin_y,
                                           &path, NULL, NULL, NULL)
            && gtk_tree_selection_path_is_selected (selection, path))
          {
            gtk_tree_path_free (path);
            continue;
          }
        if (path != NULL)
          gtk_tree_path_free (path);
        cairo_rectangle (cr, content_x, first_y + stripe * row.height,
                         allocation.width - content_x, row.height);
      }
  cairo_fill (cr);
  cairo_restore (cr);
  return GDK_EVENT_PROPAGATE;
}

static gboolean
garage_is_location_toolbar (GtkWidget *widget)
{
  GList *children;
  GList *item;
  gboolean found = FALSE;

  if (!GTK_IS_TOOLBAR (widget))
    return FALSE;

  children = gtk_container_get_children (GTK_CONTAINER (widget));
  for (item = children; item != NULL; item = item->next)
    if (g_strcmp0 (g_object_get_data (G_OBJECT (item->data), "id"),
                   "location-bar") == 0)
      {
        found = TRUE;
        break;
      }
  g_list_free (children);
  return found;
}

static GtkWidget *
garage_find_main_paned (GtkWidget *sidepane)
{
  GtkWidget *ancestor;

  for (ancestor = gtk_widget_get_parent (sidepane); ancestor != NULL;
       ancestor = gtk_widget_get_parent (ancestor))
    if (GTK_IS_PANED (ancestor))
      {
        GtkWidget *first = gtk_paned_get_child1 (GTK_PANED (ancestor));

        if (first == sidepane || gtk_widget_is_ancestor (sidepane, first))
          return ancestor;
      }
  return NULL;
}

static void
garage_own_toolbar (GarageWindowChrome *chrome)
{
  GtkWidget *main_paned;
  GtkWidget *content;
  GtkWidget *toolbar_parent;
  GtkWidget *content_shell;

  if (chrome->toolbar == NULL || chrome->sidepane == NULL
      || g_object_get_data (G_OBJECT (chrome->toolbar),
                            "garage-content-toolbar-installed") != NULL)
    return;

  /* With CSD disabled Thunar places the toolbar and the main horizontal pane
   * in separate rows of its root grid. Move the toolbar into pane two instead:
   * pane one (the sidebar) then genuinely spans the full window height, while
   * toolbar, file view and status area become one content-owned column. */
  if (gtk_widget_get_ancestor (chrome->toolbar, GTK_TYPE_HEADER_BAR) != NULL)
    return;
  main_paned = garage_find_main_paned (chrome->sidepane);
  toolbar_parent = gtk_widget_get_parent (chrome->toolbar);
  if (main_paned == NULL || toolbar_parent == NULL)
    return;

  g_object_ref (chrome->toolbar);
  gtk_container_remove (GTK_CONTAINER (toolbar_parent), chrome->toolbar);

  if (chrome->content_shell != NULL
      && gtk_paned_get_child2 (GTK_PANED (main_paned))
         == chrome->content_shell)
    {
      /* Thunar can rebuild its toolbar after a preference or plugin change.
       * Reinsert the replacement into the existing content column. */
      gtk_box_pack_start (GTK_BOX (chrome->content_shell),
                          chrome->toolbar, FALSE, FALSE, 0);
      gtk_box_reorder_child (GTK_BOX (chrome->content_shell),
                             chrome->toolbar, 0);
      gtk_widget_show (chrome->toolbar);
      g_object_unref (chrome->toolbar);
      g_object_set_data (G_OBJECT (chrome->toolbar),
                         "garage-content-toolbar-installed",
                         GINT_TO_POINTER (1));
      return;
    }

  content = gtk_paned_get_child2 (GTK_PANED (main_paned));
  if (content == NULL)
    {
      g_object_unref (chrome->toolbar);
      return;
    }
  g_object_ref (content);
  gtk_container_remove (GTK_CONTAINER (main_paned), content);

  content_shell = gtk_box_new (GTK_ORIENTATION_VERTICAL, 0);
  gtk_widget_set_hexpand (content_shell, TRUE);
  gtk_widget_set_vexpand (content_shell, TRUE);
  gtk_style_context_add_class (gtk_widget_get_style_context (content_shell),
                               "garage-content-shell");
  gtk_box_pack_start (GTK_BOX (content_shell), chrome->toolbar,
                      FALSE, FALSE, 0);
  gtk_box_pack_start (GTK_BOX (content_shell), content, TRUE, TRUE, 0);
  gtk_paned_pack2 (GTK_PANED (main_paned), content_shell, TRUE, FALSE);
  gtk_widget_show (chrome->toolbar);
  gtk_widget_show (content);
  gtk_widget_show (content_shell);
  g_object_unref (content);
  g_object_unref (chrome->toolbar);

  garage_set_widget (&chrome->content_shell, content_shell);
  g_object_set_data (G_OBJECT (chrome->toolbar),
                     "garage-content-toolbar-installed", GINT_TO_POINTER (1));
}

static void
garage_hide_compact_view (GtkWidget *widget)
{
  const gchar *id;
  GtkWidget *menu_button;
  GtkMenu *popup;
  GList *items;
  GtkWidget *compact_item;

  if (!GTK_IS_TOOL_ITEM (widget))
    return;

  id = g_object_get_data (G_OBJECT (widget), "id");
  if (g_strcmp0 (id, "view-as-compact-list") == 0)
    {
      gtk_widget_hide (widget);
      return;
    }
  if (g_strcmp0 (id, "view-switcher") != 0)
    return;

  menu_button = gtk_bin_get_child (GTK_BIN (widget));
  if (!GTK_IS_MENU_BUTTON (menu_button))
    return;
  popup = gtk_menu_button_get_popup (GTK_MENU_BUTTON (menu_button));
  if (popup == NULL)
    return;

  /* Thunar 4.20 builds this menu in icons, details, compact order. Garage
   * deliberately offers the first two and removes the redundant compact mode. */
  items = gtk_container_get_children (GTK_CONTAINER (popup));
  compact_item = g_list_nth_data (items, 2);
  if (compact_item != NULL)
    gtk_widget_hide (compact_item);
  g_list_free (items);
}

static gchar *
garage_breadcrumb_text (GObject *directory)
{
  GFile *location;
  gchar *path;
  gchar *result;
  const gchar *home;
  gsize home_length;

  if (directory == NULL || !THUNARX_IS_FILE_INFO (directory))
    return g_strdup ("");

  location = thunarx_file_info_get_location (THUNARX_FILE_INFO (directory));
  if (location == NULL)
    return g_strdup ("");
  path = g_file_get_path (location);
  if (path == NULL)
    result = g_file_get_parse_name (location);
  else
    {
      home = g_get_home_dir ();
      home_length = strlen (home);
      if (g_str_has_prefix (path, home)
          && (path[home_length] == '\0' || path[home_length] == G_DIR_SEPARATOR))
        {
          gchar *home_name = g_path_get_basename (home);
          const gchar *relative = path + home_length;
          gchar **parts;
          gchar *joined;

          while (*relative == G_DIR_SEPARATOR)
            relative++;
          parts = g_strsplit (relative, G_DIR_SEPARATOR_S, -1);
          joined = g_strjoinv (" / ", parts);
          result = *joined == '\0'
                   ? g_strdup (home_name)
                   : g_strdup_printf ("%s / %s", home_name, joined);
          g_free (joined);
          g_strfreev (parts);
          g_free (home_name);
        }
      else
        result = g_filename_display_name (path);
      g_free (path);
    }
  g_object_unref (location);
  return result;
}

static void
garage_breadcrumb_clicked (GtkButton *button, gpointer data)
{
  (void) button;
  g_signal_emit_by_name (data, "entry-requested", NULL);
}

static void
garage_unref_closure_data (gpointer data, GClosure *closure)
{
  (void) closure;
  g_object_unref (data);
}

static void
garage_install_breadcrumb (GtkWidget *location_bar)
{
  GtkWidget *child;
  GtkWidget *button;
  GtkWidget *label;
  GObject *directory = NULL;
  gchar *text;

  child = gtk_bin_get_child (GTK_BIN (location_bar));
  if (child == NULL)
    return;

  g_object_get (location_bar, "current-directory", &directory, NULL);
  text = garage_breadcrumb_text (directory);
  if (g_object_get_data (G_OBJECT (child), "garage-breadcrumb") != NULL)
    {
      label = gtk_bin_get_child (GTK_BIN (child));
      if (GTK_IS_LABEL (label))
        gtk_label_set_text (GTK_LABEL (label), text);
      g_free (text);
      if (directory != NULL)
        g_object_unref (directory);
      return;
    }

  /* Leave Thunar's temporary text entry alone while the user is editing or
   * searching. It restores the native location-buttons widget when editing
   * finishes, and the periodic installer replaces that widget with this one
   * flat, full-width breadcrumb again. */
  if (g_strcmp0 (G_OBJECT_TYPE_NAME (child), "ThunarLocationButtons") != 0)
    {
      g_free (text);
      if (directory != NULL)
        g_object_unref (directory);
      return;
    }

  if (directory != NULL)
    g_object_set (child, "current-directory", directory, NULL);
  g_object_ref (child);
  gtk_container_remove (GTK_CONTAINER (location_bar), child);

  button = gtk_button_new ();
  label = gtk_label_new (text);
  gtk_label_set_xalign (GTK_LABEL (label), 0.0f);
  gtk_label_set_ellipsize (GTK_LABEL (label), PANGO_ELLIPSIZE_MIDDLE);
  gtk_widget_set_hexpand (label, TRUE);
  gtk_widget_set_halign (label, GTK_ALIGN_FILL);
  gtk_container_add (GTK_CONTAINER (button), label);
  gtk_widget_set_hexpand (button, TRUE);
  gtk_widget_set_halign (button, GTK_ALIGN_FILL);
  gtk_widget_set_tooltip_text (button, "Click to type a location");
  gtk_style_context_add_class (gtk_widget_get_style_context (button),
                               "garage-breadcrumb");
  g_object_set_data (G_OBJECT (button), "garage-breadcrumb",
                     GINT_TO_POINTER (1));
  g_signal_connect_data (button, "clicked",
                         G_CALLBACK (garage_breadcrumb_clicked), child,
                         garage_unref_closure_data, 0);
  gtk_container_add (GTK_CONTAINER (location_bar), button);
  gtk_widget_show (label);
  gtk_widget_show (button);

  g_free (text);
  if (directory != NULL)
    g_object_unref (directory);
}

static void
garage_center_statusbar (GtkWidget *widget)
{
  GtkWidget *message_area;
  GList *children;
  GList *item;

  message_area = gtk_statusbar_get_message_area (GTK_STATUSBAR (widget));
  gtk_widget_set_hexpand (message_area, TRUE);
  gtk_widget_set_halign (message_area, GTK_ALIGN_FILL);
  children = gtk_container_get_children (GTK_CONTAINER (message_area));
  for (item = children; item != NULL; item = item->next)
    if (GTK_IS_LABEL (item->data))
      {
        gtk_label_set_xalign (GTK_LABEL (item->data), 0.5f);
        gtk_widget_set_hexpand (item->data, TRUE);
        gtk_widget_set_halign (item->data, GTK_ALIGN_FILL);
      }
  g_list_free (children);
}

static void
garage_find_widgets (GtkWidget *widget, gpointer data)
{
  GarageWindowChrome *chrome = data;
  const gchar *type_name = G_OBJECT_TYPE_NAME (widget);

  garage_hide_compact_view (widget);

  if (garage_is_shortcuts_view (widget))
    {
      if (g_object_get_data (G_OBJECT (widget), "garage-hover-installed") == NULL)
        {
          gtk_widget_add_events (widget,
                                 GDK_POINTER_MOTION_MASK | GDK_LEAVE_NOTIFY_MASK);
          g_signal_connect (widget, "motion-notify-event",
                            G_CALLBACK (garage_shortcuts_motion), NULL);
          g_signal_connect (widget, "leave-notify-event",
                            G_CALLBACK (garage_shortcuts_leave), NULL);
          g_object_set_data (G_OBJECT (widget), "garage-hover-installed",
                             GINT_TO_POINTER (1));
        }
    }
  else if (g_strcmp0 (type_name, "ThunarShortcutsPane") == 0)
    garage_set_widget (&chrome->sidepane, widget);
  else if (GTK_IS_TREE_VIEW (widget) && garage_is_standard_view (widget))
    {
      G_GNUC_BEGIN_IGNORE_DEPRECATIONS
      gtk_tree_view_set_rules_hint (GTK_TREE_VIEW (widget), TRUE);
      G_GNUC_END_IGNORE_DEPRECATIONS
      if (g_object_get_data (G_OBJECT (widget), "garage-zebra-installed") == NULL)
        {
          g_signal_connect_after (widget, "draw",
                                  G_CALLBACK (garage_details_draw_stripes), NULL);
          g_object_set_data (G_OBJECT (widget), "garage-zebra-installed",
                             GINT_TO_POINTER (1));
        }
    }
  else if (garage_is_location_toolbar (widget))
    garage_set_widget (&chrome->toolbar, widget);
  else if (g_strcmp0 (type_name, "ThunarLocationBar") == 0)
    garage_install_breadcrumb (widget);
  else if (g_strcmp0 (type_name, "ThunarStatusbar") == 0)
    garage_center_statusbar (widget);

  if (GTK_IS_CONTAINER (widget))
    gtk_container_foreach (GTK_CONTAINER (widget), garage_find_widgets, data);
}

static gboolean
garage_install (gpointer data)
{
  GList *windows = gtk_window_list_toplevels ();
  GList *item;

  (void) data;
  for (item = windows; item != NULL; item = item->next)
    {
      GtkWidget *window = item->data;
      GarageWindowChrome *chrome;

      if (g_strcmp0 (G_OBJECT_TYPE_NAME (window), "ThunarWindow") != 0)
        continue;

      chrome = g_object_get_data (G_OBJECT (window), "garage-window-chrome");
      if (chrome == NULL)
        {
          chrome = g_new0 (GarageWindowChrome, 1);
          g_object_set_data_full (G_OBJECT (window), "garage-window-chrome",
                                  chrome, garage_window_chrome_free);
        }

      garage_find_widgets (window, chrome);
      garage_own_toolbar (chrome);
    }
  g_list_free (windows);
  return G_SOURCE_CONTINUE;
}

G_MODULE_EXPORT void
gtk_module_init (gint *argc, gchar ***argv)
{
  (void) argc;
  (void) argv;
  if (g_strcmp0 (g_get_prgname (), "thunar") == 0)
    g_timeout_add (500, garage_install, NULL);
}
