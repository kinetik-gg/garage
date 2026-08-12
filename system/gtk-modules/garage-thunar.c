#include <gtk/gtk.h>

#define GARAGE_ITEM_HOVER_CLASS "garage-shortcut-item-hover"

typedef struct
{
  GtkWidget *toolbar;
  GtkWidget *sidepane;
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
  if (chrome->toolbar != NULL)
    gtk_widget_set_margin_start (chrome->toolbar, allocation->width);
}

static void
garage_find_widgets (GtkWidget *widget, gpointer data)
{
  GarageWindowChrome *chrome = data;
  const gchar *type_name = G_OBJECT_TYPE_NAME (widget);

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
          gtk_widget_get_allocation (chrome->sidepane, &allocation);
          garage_align_toolbar (chrome->sidepane, &allocation, chrome);
          g_signal_connect (chrome->sidepane, "size-allocate",
                            G_CALLBACK (garage_align_toolbar), chrome);
          g_object_set_data (G_OBJECT (chrome->sidepane),
                             "garage-toolbar-alignment-installed",
                             GINT_TO_POINTER (1));
        }
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
