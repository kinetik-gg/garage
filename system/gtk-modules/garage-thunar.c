#include <gtk/gtk.h>

#define GARAGE_ITEM_HOVER_CLASS "garage-shortcut-item-hover"

typedef struct
{
  GtkWidget *toolbar;
  GtkWidget *sidepane;
  GtkWidget *sidebar_header;
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
  garage_set_widget (&chrome->sidebar_header, NULL);
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

static void
garage_align_toolbar (GtkWidget    *sidepane,
                      GdkRectangle *allocation,
                      gpointer      data)
{
  GarageWindowChrome *chrome = data;

  (void) sidepane;
  if (chrome->sidebar_header != NULL)
    gtk_widget_set_size_request (chrome->sidebar_header,
                                 allocation->width, -1);
}

static void
garage_split_header (GarageWindowChrome *chrome)
{
  GtkWidget *headerbar;
  GtkWidget *split;
  GtkWidget *sidebar_header;
  GdkRectangle allocation;

  if (chrome->toolbar == NULL || chrome->sidepane == NULL
      || g_object_get_data (G_OBJECT (chrome->toolbar),
                            "garage-header-split-installed") != NULL)
    return;

  headerbar = gtk_widget_get_ancestor (chrome->toolbar, GTK_TYPE_HEADER_BAR);
  if (headerbar == NULL
      || gtk_header_bar_get_custom_title (GTK_HEADER_BAR (headerbar))
         != chrome->toolbar)
    return;

  /* Thunar puts one toolbar across its whole CSD header, independently of the
   * paned content below. Give the header the same two-column structure as the
   * body: a calm sidebar-colored spacer and the real toolbar over the file
   * content. Keeping the original toolbar intact preserves every action and
   * the location entry's behaviour. */
  g_object_ref (chrome->toolbar);
  gtk_header_bar_set_custom_title (GTK_HEADER_BAR (headerbar), NULL);

  split = gtk_box_new (GTK_ORIENTATION_HORIZONTAL, 0);
  gtk_widget_set_hexpand (split, TRUE);
  gtk_widget_set_vexpand (split, TRUE);
  gtk_style_context_add_class (gtk_widget_get_style_context (split),
                               "garage-header-split");

  sidebar_header = gtk_box_new (GTK_ORIENTATION_HORIZONTAL, 0);
  gtk_widget_set_vexpand (sidebar_header, TRUE);
  gtk_style_context_add_class (gtk_widget_get_style_context (sidebar_header),
                               "garage-sidebar-header");
  gtk_box_pack_start (GTK_BOX (split), sidebar_header, FALSE, FALSE, 0);
  gtk_box_pack_start (GTK_BOX (split), chrome->toolbar, TRUE, TRUE, 0);
  gtk_header_bar_set_custom_title (GTK_HEADER_BAR (headerbar), split);
  /* Do not use show_all(): Thunar intentionally keeps most toolbar actions
   * hidden according to last-toolbar-items, and recursively showing the split
   * would resurrect all of that clutter. */
  gtk_widget_show (sidebar_header);
  gtk_widget_show (chrome->toolbar);
  gtk_widget_show (split);
  g_object_unref (chrome->toolbar);

  garage_set_widget (&chrome->sidebar_header, sidebar_header);
  gtk_widget_get_allocation (chrome->sidepane, &allocation);
  garage_align_toolbar (chrome->sidepane, &allocation, chrome);
  g_object_set_data (G_OBJECT (chrome->toolbar),
                     "garage-header-split-installed", GINT_TO_POINTER (1));
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
  else if (GTK_IS_TOOLBAR (widget)
           && gtk_widget_get_ancestor (widget, GTK_TYPE_HEADER_BAR) != NULL)
    garage_set_widget (&chrome->toolbar, widget);

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
      GdkRectangle allocation;

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
      if (chrome->toolbar != NULL && chrome->sidepane != NULL
          && g_object_get_data (G_OBJECT (chrome->sidepane),
                                "garage-toolbar-alignment-installed") == NULL)
        {
          garage_split_header (chrome);
          gtk_widget_get_allocation (chrome->sidepane, &allocation);
          garage_align_toolbar (chrome->sidepane, &allocation, chrome);
          g_signal_connect (chrome->sidepane, "size-allocate",
                            G_CALLBACK (garage_align_toolbar), chrome);
          g_object_set_data (G_OBJECT (chrome->sidepane),
                             "garage-toolbar-alignment-installed",
                             GINT_TO_POINTER (1));
        }
      else
        garage_split_header (chrome);
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
