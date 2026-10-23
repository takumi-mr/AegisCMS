import React, { useState, useEffect, useMemo } from 'react';
import {
  Calendar as CalendarIcon,
  ChevronLeft,
  ChevronRight,
  List,
  CalendarDays,
  ExternalLink,
  Clock,
  CheckCircle2,
  CalendarClock,
  X,
  RefreshCw,
  Edit,
  Trash2,
} from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { aegisApi } from '../api/client';
import type { ScheduleItem } from '../types';
import { cx } from '../utils/cx';
import { formatDateTime, toDateTimeLocal } from '../utils/format';
import { Button } from './ui/Button';
import styles from './ScheduleCalendar.module.css';

interface ScheduleCalendarProps {
  onOpenContent?: (contentType: string, id: string) => void;
  onShowToast: (message: string, type?: 'success' | 'error') => void;
}

type ViewMode = 'calendar' | 'agenda';
type EventFilter = 'all' | 'scheduled_publish' | 'scheduled_unpublish' | 'published';

export const ScheduleCalendar: React.FC<ScheduleCalendarProps> = ({
  onOpenContent,
  onShowToast,
}) => {
  const { t, i18n } = useTranslation();
  const [items, setItems] = useState<ScheduleItem[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [currentDate, setCurrentDate] = useState<Date>(() => new Date());
  const [viewMode, setViewMode] = useState<ViewMode>('calendar');
  const [contentTypeFilter, setContentTypeFilter] = useState<string>('all');
  const [eventFilter, setEventFilter] = useState<EventFilter>('all');
  const [selectedItem, setSelectedItem] = useState<ScheduleItem | null>(null);

  // Quick edit unpublish date state inside detail modal
  const [modalUnpublishAt, setModalUnpublishAt] = useState<string>('');
  const [isUpdatingUnpublish, setIsUpdatingUnpublish] = useState<boolean>(false);

  const fetchSchedule = async () => {
    setLoading(true);
    try {
      const typeParam = contentTypeFilter === 'all' ? undefined : contentTypeFilter;
      const res = await aegisApi.getSchedule(typeParam);
      setItems(res);
    } catch (err: any) {
      console.error('Failed to load schedule:', err);
      onShowToast(err.message || t('schedule.loadFailed'), 'error');
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchSchedule();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contentTypeFilter]);

  // Available content types extracted from fetched items or defaults
  const contentTypes = useMemo(() => {
    const set = new Set<string>(['article', 'product']);
    for (const item of items) {
      set.add(item.content_type);
    }
    return Array.from(set);
  }, [items]);

  // Filter items by event filter
  const filteredItems = useMemo(() => {
    return items.filter((item) => {
      if (eventFilter === 'all') return true;
      if (eventFilter === 'scheduled_publish') {
        return item.status === 'scheduled';
      }
      if (eventFilter === 'scheduled_unpublish') {
        return Boolean(item.unpublish_at);
      }
      if (eventFilter === 'published') {
        return item.status === 'published';
      }
      return true;
    });
  }, [items, eventFilter]);

  // Calendar month navigation
  const prevMonth = () => {
    setCurrentDate(new Date(currentDate.getFullYear(), currentDate.getMonth() - 1, 1));
  };

  const nextMonth = () => {
    setCurrentDate(new Date(currentDate.getFullYear(), currentDate.getMonth() + 1, 1));
  };

  const gotoToday = () => {
    setCurrentDate(new Date());
  };

  // Calendar grid calculations
  const year = currentDate.getFullYear();
  const month = currentDate.getMonth(); // 0-indexed

  const monthName = useMemo(() => {
    return new Intl.DateTimeFormat(i18n.language || 'ja', {
      year: 'numeric',
      month: 'long',
    }).format(currentDate);
  }, [currentDate, i18n.language]);

  const weekdays = useMemo(() => {
    const baseDate = new Date(2023, 0, 1); // Sunday Jan 1 2023
    const names: string[] = [];
    for (let i = 0; i < 7; i++) {
      const d = new Date(baseDate);
      d.setDate(baseDate.getDate() + i);
      names.push(new Intl.DateTimeFormat(i18n.language || 'ja', { weekday: 'short' }).format(d));
    }
    return names;
  }, [i18n.language]);

  interface DayCellData {
    date: Date;
    isCurrentMonth: boolean;
    isToday: boolean;
    dateKey: string; // YYYY-MM-DD
  }

  const calendarDays = useMemo(() => {
    const firstDayOfMonth = new Date(year, month, 1);
    const startDayOfWeek = firstDayOfMonth.getDay(); // 0 (Sun) to 6 (Sat)
    const lastDayOfMonth = new Date(year, month + 1, 0);
    const totalDaysInMonth = lastDayOfMonth.getDate();

    const today = new Date();
    const todayYear = today.getFullYear();
    const todayMonth = today.getMonth();
    const todayDate = today.getDate();

    const days: DayCellData[] = [];

    // Leading days from previous month
    const prevMonthLastDay = new Date(year, month, 0).getDate();
    for (let i = startDayOfWeek - 1; i >= 0; i--) {
      const d = new Date(year, month - 1, prevMonthLastDay - i);
      const isToday =
        d.getFullYear() === todayYear && d.getMonth() === todayMonth && d.getDate() === todayDate;
      const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
      days.push({
        date: d,
        isCurrentMonth: false,
        isToday,
        dateKey: key,
      });
    }

    // Days in current month
    for (let i = 1; i <= totalDaysInMonth; i++) {
      const d = new Date(year, month, i);
      const isToday =
        d.getFullYear() === todayYear && d.getMonth() === todayMonth && d.getDate() === todayDate;
      const key = `${year}-${String(month + 1).padStart(2, '0')}-${String(i).padStart(2, '0')}`;
      days.push({
        date: d,
        isCurrentMonth: true,
        isToday,
        dateKey: key,
      });
    }

    // Trailing days to fill 5 or 6 weeks (multiples of 7, up to 35 or 42 cells)
    const remainder = days.length % 7;
    if (remainder > 0) {
      const fillCount = 7 - remainder;
      for (let i = 1; i <= fillCount; i++) {
        const d = new Date(year, month + 1, i);
        const isToday =
          d.getFullYear() === todayYear && d.getMonth() === todayMonth && d.getDate() === todayDate;
        const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
        days.push({
          date: d,
          isCurrentMonth: false,
          isToday,
          dateKey: key,
        });
      }
    }

    return days;
  }, [year, month]);

  // Group events by day key (YYYY-MM-DD)
  // An item can have both a publish date and an unpublish date!
  interface CalendarEvent {
    item: ScheduleItem;
    type: 'publish' | 'unpublish';
    date: Date;
    formattedTime: string;
  }

  const eventsByDate = useMemo(() => {
    const map = new Map<string, CalendarEvent[]>();

    const addEvent = (key: string, ev: CalendarEvent) => {
      const existing = map.get(key) || [];
      existing.push(ev);
      map.set(key, existing);
    };

    for (const item of filteredItems) {
      // 1. Publish event (if scheduled or published)
      if (item.published_at) {
        const d = new Date(item.published_at);
        if (!Number.isNaN(d.getTime())) {
          const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
          const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
          addEvent(key, { item, type: 'publish', date: d, formattedTime: time });
        }
      }

      // 2. Unpublish event (if set)
      if (item.unpublish_at) {
        const d = new Date(item.unpublish_at);
        if (!Number.isNaN(d.getTime())) {
          const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`;
          const time = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
          addEvent(key, { item, type: 'unpublish', date: d, formattedTime: time });
        }
      }
    }

    // Sort events inside each day by time
    for (const [, list] of map) {
      list.sort((a, b) => a.date.getTime() - b.date.getTime());
    }

    return map;
  }, [filteredItems]);

  const handleOpenDetail = (item: ScheduleItem) => {
    setSelectedItem(item);
    setModalUnpublishAt(item.unpublish_at ? toDateTimeLocal(item.unpublish_at) : '');
  };

  const handleSaveModalUnpublish = async () => {
    if (!selectedItem) return;
    setIsUpdatingUnpublish(true);
    try {
      const unpublishIso = modalUnpublishAt ? new Date(modalUnpublishAt).toISOString() : null;

      // Validate that unpublish date is after published_at
      if (unpublishIso && selectedItem.published_at) {
        if (new Date(unpublishIso).getTime() <= new Date(selectedItem.published_at).getTime()) {
          onShowToast(t('schedule.errors.unpublishBeforePublish'), 'error');
          setIsUpdatingUnpublish(false);
          return;
        }
      }

      // Update content via API
      await aegisApi.updateContent(selectedItem.content_type, selectedItem.id, {
        unpublish_at: unpublishIso,
      });

      onShowToast(t('schedule.unpublishUpdated'));
      // Update local item
      const updated = { ...selectedItem, unpublish_at: unpublishIso };
      setSelectedItem(updated);
      setItems((prev) => prev.map((p) => (p.id === selectedItem.id ? updated : p)));
    } catch (err: any) {
      onShowToast(err.message || t('schedule.unpublishUpdateFailed'), 'error');
    } finally {
      setIsUpdatingUnpublish(false);
    }
  };

  const handleClearModalUnpublish = async () => {
    if (!selectedItem) return;
    setIsUpdatingUnpublish(true);
    try {
      await aegisApi.updateContent(selectedItem.content_type, selectedItem.id, {
        unpublish_at: null,
      });
      onShowToast(t('schedule.unpublishCleared'));
      const updated = { ...selectedItem, unpublish_at: null };
      setSelectedItem(updated);
      setModalUnpublishAt('');
      setItems((prev) => prev.map((p) => (p.id === selectedItem.id ? updated : p)));
    } catch (err: any) {
      onShowToast(err.message || t('schedule.unpublishUpdateFailed'), 'error');
    } finally {
      setIsUpdatingUnpublish(false);
    }
  };

  return (
    <div className={styles.container}>
      <div className={styles.column}>
        {/* Header */}
        <div className={styles.header}>
          <div className={styles.titleArea}>
            <CalendarIcon className={styles.titleIcon} size={26} />
            <h1 className={styles.titleText}>{t('schedule.title')}</h1>
            <span className={styles.countBadge}>
              {t('schedule.itemCount', { count: filteredItems.length })}
            </span>
          </div>

          <div className={styles.controlsGroup}>
            <Button variant="tool" onClick={fetchSchedule} disabled={loading}>
              <RefreshCw className={loading ? 'animate-spin' : ''} size={14} />
              <span>{t('common.reload')}</span>
            </Button>

            {/* View Mode Switcher */}
            <div className={styles.viewTabs} role="tablist">
              <button
                type="button"
                role="tab"
                aria-selected={viewMode === 'calendar'}
                className={cx(styles.viewTab, viewMode === 'calendar' && styles.activeViewTab)}
                onClick={() => setViewMode('calendar')}
              >
                <CalendarDays size={14} />
                <span>{t('schedule.views.calendar')}</span>
              </button>
              <button
                type="button"
                role="tab"
                aria-selected={viewMode === 'agenda'}
                className={cx(styles.viewTab, viewMode === 'agenda' && styles.activeViewTab)}
                onClick={() => setViewMode('agenda')}
              >
                <List size={14} />
                <span>{t('schedule.views.agenda')}</span>
              </button>
            </div>
          </div>
        </div>

        {/* Toolbar: Navigation and Filters */}
        <div className={styles.toolbar}>
          <div className={styles.navGroup}>
            {viewMode === 'calendar' && (
              <>
                <Button variant="tool" onClick={prevMonth} aria-label={t('schedule.prevMonth')}>
                  <ChevronLeft size={16} />
                </Button>
                <span className={styles.currentMonth}>{monthName}</span>
                <Button variant="tool" onClick={nextMonth} aria-label={t('schedule.nextMonth')}>
                  <ChevronRight size={16} />
                </Button>
                <Button variant="tool" onClick={gotoToday}>
                  <span>{t('schedule.today')}</span>
                </Button>
              </>
            )}
            {viewMode === 'agenda' && (
              <span className={styles.currentMonth}>{t('schedule.agendaTitle')}</span>
            )}
          </div>

          <div className={styles.controlsGroup}>
            {/* Content Type Filter */}
            <select
              className={styles.filterSelect}
              value={contentTypeFilter}
              onChange={(e) => setContentTypeFilter(e.target.value)}
              aria-label={t('schedule.filters.contentType')}
            >
              <option value="all">{t('schedule.filters.allTypes')}</option>
              {contentTypes.map((type) => (
                <option key={type} value={type}>
                  {type === 'article'
                    ? t('contentList.articles')
                    : type === 'product'
                      ? t('header.tabs.products')
                      : type}
                </option>
              ))}
            </select>

            {/* Event Filter */}
            <select
              className={styles.filterSelect}
              value={eventFilter}
              onChange={(e) => setEventFilter(e.target.value as EventFilter)}
              aria-label={t('schedule.filters.eventType')}
            >
              <option value="all">{t('schedule.filters.allEvents')}</option>
              <option value="scheduled_publish">{t('schedule.filters.scheduledPublish')}</option>
              <option value="scheduled_unpublish">
                {t('schedule.filters.scheduledUnpublish')}
              </option>
              <option value="published">{t('schedule.filters.published')}</option>
            </select>
          </div>
        </div>

        {/* Calendar View */}
        {viewMode === 'calendar' && (
          <div className={styles.calendarWrapper}>
            {/* Weekday headers */}
            <div className={styles.weekHeaderRow}>
              {weekdays.map((day, idx) => (
                <div key={idx} className={styles.weekHeaderCell}>
                  {day}
                </div>
              ))}
            </div>

            {/* Days grid */}
            <div className={styles.monthGrid}>
              {calendarDays.map((day) => {
                const dayEvents = eventsByDate.get(day.dateKey) || [];
                return (
                  <div
                    key={day.dateKey}
                    className={cx(
                      styles.dayCell,
                      !day.isCurrentMonth && styles.otherMonth,
                      day.isToday && styles.isToday
                    )}
                  >
                    <div className={dayHeaderStyle(styles, day.isToday)}>
                      <span className={styles.dayNumber}>{day.date.getDate()}</span>
                    </div>

                    <div className={styles.eventsContainer}>
                      {dayEvents.slice(0, 3).map((ev, idx) => {
                        const isUnpublish = ev.type === 'unpublish';
                        const isScheduled = !isUnpublish && ev.item.status === 'scheduled';
                        const pillClass = isUnpublish
                          ? styles.pillUnpublish
                          : isScheduled
                            ? styles.pillScheduled
                            : styles.pillPublished;

                        return (
                          <button
                            key={idx}
                            type="button"
                            className={cx(styles.eventPill, pillClass)}
                            onClick={() => handleOpenDetail(ev.item)}
                            title={`${ev.formattedTime} [${ev.item.content_type}] ${ev.item.title} (${isUnpublish ? t('schedule.unpublishAt') : t('schedule.publishAt')})`}
                          >
                            <span className={styles.pillTime}>{ev.formattedTime}</span>
                            {isUnpublish ? (
                              <Clock size={10} />
                            ) : isScheduled ? (
                              <CalendarClock size={10} />
                            ) : (
                              <CheckCircle2 size={10} />
                            )}
                            <span className={styles.pillTitle}>{ev.item.title}</span>
                          </button>
                        );
                      })}

                      {dayEvents.length > 3 && (
                        <span
                          className={styles.moreCount}
                          onClick={() => handleOpenDetail(dayEvents[3].item)}
                        >
                          +{dayEvents.length - 3} {t('schedule.more')}
                        </span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Agenda View */}
        {viewMode === 'agenda' && (
          <div className={styles.agendaWrapper}>
            {filteredItems.length === 0 ? (
              <div className={styles.agendaEmpty}>
                <CalendarIcon size={36} className={styles.agendaEmptyIcon} />
                <p>{t('schedule.noItems')}</p>
              </div>
            ) : (
              filteredItems.map((item) => (
                <div
                  key={item.id}
                  className={styles.agendaCard}
                  onClick={() => handleOpenDetail(item)}
                >
                  <div className={styles.cardMain}>
                    <div className={styles.cardTop}>
                      <span className={styles.cardTitle}>{item.title}</span>
                      <span className={styles.cardTypeBadge}>{item.content_type}</span>
                      <span className={styles.cardSlug}>/{item.slug}</span>
                    </div>

                    <div className={styles.cardScheduleRow}>
                      {item.published_at && (
                        <div
                          className={cx(
                            styles.scheduleChip,
                            item.status === 'scheduled'
                              ? styles.chipScheduled
                              : styles.chipPublished
                          )}
                        >
                          {item.status === 'scheduled' ? (
                            <CalendarClock size={14} />
                          ) : (
                            <CheckCircle2 size={14} />
                          )}
                          <span>
                            {item.status === 'scheduled'
                              ? t('schedule.scheduledFor', {
                                  date: formatDateTime(item.published_at),
                                })
                              : t('schedule.publishedAt', {
                                  date: formatDateTime(item.published_at),
                                })}
                          </span>
                        </div>
                      )}

                      {item.unpublish_at && (
                        <div className={cx(styles.scheduleChip, styles.chipUnpublish)}>
                          <Clock size={14} />
                          <span>
                            {t('schedule.unpublishFor', {
                              date: formatDateTime(item.unpublish_at),
                            })}
                          </span>
                        </div>
                      )}
                    </div>
                  </div>

                  <div className={styles.cardActions}>
                    {onOpenContent && (
                      <Button
                        variant="tool"
                        onClick={(e) => {
                          e.stopPropagation();
                          onOpenContent(item.content_type, item.id);
                        }}
                        title={t('schedule.openInEditor')}
                      >
                        <Edit size={14} />
                        <span>{t('schedule.edit')}</span>
                      </Button>
                    )}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>

      {/* Detail & Quick Edit Modal */}
      {selectedItem && (
        <div className={styles.modalBackdrop} onClick={() => setSelectedItem(null)}>
          <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
            <div className={styles.modalHeader}>
              <h2 className={styles.modalTitle}>{selectedItem.title}</h2>
              <button
                type="button"
                className={styles.closeButton}
                onClick={() => setSelectedItem(null)}
                aria-label={t('common.close')}
              >
                <X size={18} />
              </button>
            </div>

            <div className={styles.modalBody}>
              <div className={styles.infoRow}>
                <span className={styles.infoLabel}>{t('schedule.details.contentType')}</span>
                <span className={styles.infoValue}>{selectedItem.content_type}</span>
              </div>

              <div className={styles.infoRow}>
                <span className={styles.infoLabel}>{t('schedule.details.slug')}</span>
                <span className={styles.infoValue}>/{selectedItem.slug}</span>
              </div>

              <div className={styles.infoRow}>
                <span className={styles.infoLabel}>{t('schedule.details.status')}</span>
                <span className={styles.infoValue}>
                  {t(`common.status.${selectedItem.status}`, {
                    defaultValue: selectedItem.status,
                  })}
                </span>
              </div>

              <div className={styles.scheduleBox}>
                <div className={styles.infoRow}>
                  <span className={styles.infoLabel}>{t('schedule.details.publishDate')}</span>
                  <span className={styles.infoValue}>
                    {selectedItem.published_at
                      ? formatDateTime(selectedItem.published_at)
                      : t('schedule.details.notSet')}
                  </span>
                </div>

                <div className={styles.unpublishSettingRow}>
                  <span className={styles.infoLabel}>{t('schedule.details.unpublishDate')}</span>
                  <div className={styles.unpublishInputRow}>
                    <input
                      type="datetime-local"
                      className={styles.dateTimeInput}
                      value={modalUnpublishAt}
                      onChange={(e) => setModalUnpublishAt(e.target.value)}
                      aria-label={t('schedule.details.unpublishDate')}
                    />
                    <Button
                      variant="primary"
                      onClick={handleSaveModalUnpublish}
                      disabled={isUpdatingUnpublish}
                    >
                      {isUpdatingUnpublish ? (
                        <RefreshCw className="animate-spin" size={14} />
                      ) : (
                        <span>{t('common.save')}</span>
                      )}
                    </Button>
                    {selectedItem.unpublish_at && (
                      <Button
                        variant="tool"
                        onClick={handleClearModalUnpublish}
                        disabled={isUpdatingUnpublish}
                        title={t('schedule.details.clearUnpublish')}
                      >
                        <Trash2 size={14} />
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            </div>

            <div className={styles.modalFooter}>
              <div>
                {onOpenContent && (
                  <Button
                    variant="tool"
                    onClick={() => {
                      const cType = selectedItem.content_type;
                      const id = selectedItem.id;
                      setSelectedItem(null);
                      onOpenContent(cType, id);
                    }}
                  >
                    <ExternalLink size={14} />
                    <span>{t('schedule.openInEditor')}</span>
                  </Button>
                )}
              </div>
              <Button onClick={() => setSelectedItem(null)}>{t('common.close')}</Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

function dayHeaderStyle(s: Record<string, string>, isToday: boolean): string {
  return cx(s.dayHeader, isToday && s.isToday);
}
